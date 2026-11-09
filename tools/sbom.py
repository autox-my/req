#!/usr/bin/env python3
"""tools/sbom.py: the software bill of materials of the req binary, as CycloneDX 1.5 JSON (ADR 0036).

    python3 tools/sbom.py --out docs/sbom.cdx.json       write it
    python3 tools/sbom.py --check docs/sbom.cdx.json     is the committed file current?

What it lists: every crate in the dependency graph of the two release targets (ADR 0025). The graph is
`cargo tree` (the features and targets Cargo actually activates; `cargo metadata` lists optional dependencies
that no feature enables, which would put crates like `bytes` and `rand` in a bill of materials for a binary
that does not contain them), the licence and repository come from `cargo metadata`, and the SHA-256 of each
registry crate from `Cargo.lock`. It adds no tool to the toolchain and reads nothing from the network.

`scope` says what the crate is *for*, because "in the lock" and "in the binary" are different facts:
  required  linked into the binary: reachable from `req` through normal dependencies only;
  excluded  used to build it (build scripts, procedural macros, and what those pull in) and not shipped.
A dev-dependency is not listed at all. Procedural macros are compile-time code even though Cargo records them
as normal dependencies; `cargo tree --edges no-proc-macro` is what separates them.

Deterministic on purpose: no timestamp; the serial number is a UUIDv5 of the lock file's SHA-256; components
and edges are sorted. The same lock, the same `req` version and the same metadata give the same bytes, so the
committed file can be drift-checked like `docs/spec.md`: a change to a dependency, a version or `Cargo.lock`
makes `--check` fail until the file is regenerated in the same commit.

Exit: 0 written / current; 1 stale (`--check`); 2 usage; 3 UNVERIFIABLE (cargo or the lock cannot be read:
a graph that could not be read is not a graph that matched).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
import uuid
from pathlib import Path

TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-apple-darwin")
SPEC_VERSION = "1.5"
TOOL_NAME = "tools/sbom.py"


class Unverifiable(Exception):
    """The inputs could not be read; the verdict is exit 3, never a pass."""


def purl(name: str, version: str) -> str:
    return f"pkg:cargo/{name}@{version}"


def load_lock(path: Path) -> tuple[dict[tuple[str, str], str], str]:
    """(name, version) -> sha256 of every registry crate in the lock, and the lock file's own sha256."""
    try:
        raw = path.read_bytes()
        lock = tomllib.loads(raw.decode("utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as err:
        raise Unverifiable(f"cannot read {path}: {err}") from err
    checksums = {}
    for package in lock.get("package", []):
        if "checksum" in package:
            checksums[(package["name"], package["version"])] = package["checksum"]
    return checksums, hashlib.sha256(raw).hexdigest()


def run_cargo(root: Path, args: list[str]) -> str:
    cargo = os.environ.get("CARGO", "cargo")
    try:
        done = subprocess.run([cargo, *args], cwd=root, capture_output=True, text=True, check=False)
    except OSError as err:
        raise Unverifiable(f"cannot run {cargo}: {err}") from err
    if done.returncode != 0:
        raise Unverifiable(f"`cargo {' '.join(args)}` failed: {done.stderr.strip()[-400:]}")
    return done.stdout


Node = tuple[str, str]  # (name, version)
LINE = re.compile(r"^(\d+)(\S+) v(\S+)")


def parse_tree(text: str) -> tuple[Node | None, dict[Node, set[Node]]]:
    """The root and the parent -> children edges of `cargo tree --prefix depth` output.

    A node already printed is shown again with `(*)` and no children, so every node's children appear once,
    under its first occurrence: the stack of the current ancestors is enough to read the edges.
    """
    root: Node | None = None
    edges: dict[Node, set[Node]] = {}
    stack: list[Node] = []
    for line in text.splitlines():
        match = LINE.match(line)
        if not match:
            continue
        depth, name, version = int(match.group(1)), match.group(2), match.group(3)
        node = (name, version)
        del stack[depth:]
        if depth == 0:
            root = node
        elif stack:
            edges.setdefault(stack[-1], set()).add(node)
        edges.setdefault(node, set())
        stack.append(node)
    return root, edges


def cargo_graph(root: Path, target: str, edges: str) -> tuple[Node | None, dict[Node, set[Node]]]:
    out = run_cargo(root, ["tree", "--locked", "--offline", "--target", target, "-e", edges,
                           "--prefix", "depth", "--format", "{p}"])
    return parse_tree(out)


def cargo_packages(root: Path) -> dict[Node, dict]:
    out = run_cargo(root, ["metadata", "--format-version", "1", "--locked", "--offline"])
    try:
        doc = json.loads(out)
    except json.JSONDecodeError as err:
        raise Unverifiable(f"`cargo metadata` printed no JSON: {err}") from err
    return {(p["name"], p["version"]): p for p in doc["packages"]}


def reachable(edges: dict[Node, set[Node]], root: Node) -> set[Node]:
    seen = {root}
    stack = [root]
    while stack:
        for child in edges.get(stack.pop(), ()):
            if child not in seen:
                seen.add(child)
                stack.append(child)
    return seen


def component(package: dict, checksums: dict[tuple[str, str], str], scope: str) -> dict:
    name, version = package["name"], package["version"]
    out = {"type": "library", "bom-ref": purl(name, version), "name": name, "version": version, "scope": scope}
    if package.get("license"):
        out["licenses"] = [{"expression": package["license"]}]
    out["purl"] = purl(name, version)
    checksum = checksums.get((name, version))
    if checksum:
        out["hashes"] = [{"alg": "SHA-256", "content": checksum}]
    if package.get("repository"):
        out["externalReferences"] = [{"type": "vcs", "url": package["repository"]}]
    return out


def build_bom(
    root: Node,
    packages: dict[Node, dict],
    build_edges: dict[Node, set[Node]],
    shipped: set[Node],
    checksums: dict[Node, str],
    lock_sha: str,
    targets: list[str],
) -> dict:
    members = sorted(reachable(build_edges, root) - {root})
    for node in [root, *members]:
        if node not in packages:
            raise Unverifiable(f"{node[0]} {node[1]} is in the graph but not in `cargo metadata`")

    components = [component(packages[n], checksums, "required" if n in shipped else "excluded") for n in members]
    dependencies = []
    for node in sorted(reachable(build_edges, root)):
        children = sorted(purl(*c) for c in build_edges.get(node, ()))
        dependencies.append({"ref": purl(*node), "dependsOn": children})

    top = packages[root]
    app = {
        "type": "application",
        "bom-ref": purl(*root),
        "name": top["name"],
        "version": top["version"],
        "purl": purl(*root),
    }
    if top.get("license"):
        app["licenses"] = [{"expression": top["license"]}]
    if top.get("repository"):
        app["externalReferences"] = [{"type": "vcs", "url": top["repository"]}]

    serial = uuid.uuid5(uuid.NAMESPACE_URL, f"req-sbom:{lock_sha}")
    return {
        "bomFormat": "CycloneDX",
        "specVersion": SPEC_VERSION,
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "tools": {"components": [{"type": "application", "name": TOOL_NAME}]},
            "component": app,
            "properties": [
                {"name": "req:targets", "value": ",".join(targets)},
                {"name": "req:lock-sha256", "value": lock_sha},
                {
                    "name": "req:scope",
                    "value": "required = linked into the binary; excluded = used to build it (build scripts, "
                    "procedural macros), not shipped; dev-dependencies are not listed",
                },
            ],
        },
        "components": components,
        "dependencies": dependencies,
    }


def render(bom: dict) -> str:
    return json.dumps(bom, indent=2, ensure_ascii=False) + "\n"


def generate(root: Path, targets: list[str]) -> str:
    checksums, lock_sha = load_lock(root / "Cargo.lock")
    top: Node | None = None
    build_edges: dict[Node, set[Node]] = {}
    shipped: set[Node] = set()
    for target in targets:
        for kind, into_build in (("normal,build", True), ("normal,no-proc-macro", False)):
            this_root, edges = cargo_graph(root, target, kind)
            if this_root is None:
                raise Unverifiable(f"`cargo tree` for {target} printed no root package")
            if top is not None and top != this_root:
                raise Unverifiable("`cargo tree` disagrees with itself about the root package")
            top = this_root
            if into_build:
                for parent, children in edges.items():
                    build_edges.setdefault(parent, set()).update(children)
            else:
                shipped |= reachable(edges, this_root)
    assert top is not None
    return render(build_bom(top, cargo_packages(root), build_edges, shipped, checksums, lock_sha, targets))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="sbom.py", description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--out", metavar="FILE", help="write the SBOM to FILE")
    mode.add_argument("--check", metavar="FILE", help="exit 1 unless FILE is what would be written now")
    parser.add_argument("--root", default=".", help="the repository root (Cargo.lock, Cargo.toml); default .")
    parser.add_argument("--target", action="append", dest="targets", help="a release target (default: both)")
    try:
        args = parser.parse_args(argv)
    except SystemExit as exit_:
        return 2 if exit_.code else 0
    targets = args.targets or list(TARGETS)
    root = Path(args.root)

    try:
        text = generate(root, targets)
    except Unverifiable as err:
        print(f"sbom: UNVERIFIABLE: {err}", file=sys.stderr)
        return 3

    if args.out:
        Path(args.out).write_text(text, encoding="utf-8")
        print(f"sbom: wrote {args.out}")
        return 0

    path = Path(args.check)
    try:
        current = path.read_text(encoding="utf-8")
    except OSError as err:
        print(f"sbom: STALE: cannot read {path}: {err}", file=sys.stderr)
        return 1
    if current == text:
        print(f"sbom: {path} is CURRENT")
        return 0
    have = {c["bom-ref"] for c in json.loads(current).get("components", [])} if current.strip().startswith("{") else set()
    now = {c["bom-ref"] for c in json.loads(text)["components"]}
    print(f"sbom: STALE: {path} is not what the dependency graph and Cargo.lock give now", file=sys.stderr)
    for name in sorted(now - have):
        print(f"  added   {name}", file=sys.stderr)
    for name in sorted(have - now):
        print(f"  removed {name}", file=sys.stderr)
    print(f"  regenerate it: python3 tools/sbom.py --out {path}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

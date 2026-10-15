#!/usr/bin/env python3
"""Reconciliation check: a `.forgejo/` diff is actually reviewed in the blocking path.

Why this exists. `review/rule.json` carries a `.forgejo/**` checklist and
`.forgejo/workflows/review.yml` is the merge-blocking review. The two are wired together
by a single string — review.yml's `--exclude` — and nothing about a rule file's presence
proves that the file it covers ever reaches the model. This CLI **silently** falls back
to the built-in rubric for a path it does not match, so a `.forgejo/**` checklist filtered
out upstream looks exactly like a checklist that fired: every exit code stays zero. The
*effect* is therefore checked here, never the exit status.

Driving the step, not the rules. The exclude value is read out of
`.forgejo/workflows/review.yml` and used to run the same `ocr review` invocation the
workflow runs, so editing the workflow changes this check's behaviour — a `.forgejo/**`
entry added back to `--exclude` turns this check red again.

Nothing leaves the machine: the provider is a stub bound to 127.0.0.1, and it records
every request body verbatim so the assertions are about what the model actually received.

The rule file is the **checkout's** `review/rule.json`, not `origin/main`'s. The workflow
takes its rules from `origin/main` (that trust boundary is review.yml's own `git cat-file`
probe, tested there); what is under test here is the branch's own rules reaching the model.

Exit codes: 0 the `.forgejo` path reached the model under the custom checklist, the bench
corpus did not, and the control file did; 1 a defect; 3 could not answer (no `ocr`, or the
review never reached the stub).
"""
import http.server
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".forgejo" / "workflows" / "review.yml"
RULE_FILE = ROOT / "review" / "rule.json"

FORGEJO_PATH = ".forgejo/workflows/review.yml"
BENCH_PATH = "bench/corpus/scope-fixture.yml"
CONTROL_PATH = "src/scope_fixture.rs"


class Stub(http.server.BaseHTTPRequestHandler):
    """Records request bodies and answers `task_done`, so the review ends in one round."""

    protocol_version = "HTTP/1.1"
    bodies: list = []

    def _read(self) -> bytes:
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else b""
        type(self).bodies.append(body.decode("utf-8", "replace"))
        return body

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        body = self._read()
        try:
            want_tools = bool(json.loads(body).get("tools"))
        except Exception:
            want_tools = True
        if want_tools:
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "task_done",
                            "arguments": json.dumps({"state": "DONE"}),
                        },
                    }
                ],
            }
            choice = {"index": 0, "finish_reason": "tool_calls", "message": message}
        else:
            choice = {
                "index": 0,
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "[]"},
            }
        self._send(
            json.dumps(
                {
                    "id": "chatcmpl-stub",
                    "object": "chat.completion",
                    "created": 0,
                    "model": "stub",
                    "choices": [choice],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
                }
            ).encode()
        )

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        self._read()
        self._send(b'{"data":[{"id":"stub-model"}]}')

    def _send(self, payload: bytes) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args) -> None:
        pass


def git(cwd: pathlib.Path, *args: str) -> str:
    # A throwaway fixture repo under a temp dir — never this repository's history. The
    # placeholder identity is set through the environment and is scoped to these calls.
    env = os.environ.copy()
    env.update(
        GIT_AUTHOR_NAME="review-scope", GIT_AUTHOR_EMAIL="review-scope@example.invalid",
        GIT_COMMITTER_NAME="review-scope", GIT_COMMITTER_EMAIL="review-scope@example.invalid",
    )
    return subprocess.run(
        ["git", *args], cwd=cwd, env=env, capture_output=True, text=True, check=True
    ).stdout.strip()


def build_fixture(tmp: pathlib.Path) -> tuple[str, str]:
    """A base commit and a head commit touching .forgejo, the bench corpus and a control."""
    repo = tmp / "fixture"
    (repo / ".forgejo" / "workflows").mkdir(parents=True)
    (repo / "bench" / "corpus").mkdir(parents=True)
    (repo / "src").mkdir()

    (repo / ".forgejo" / "workflows" / "review.yml").write_text(
        "name: OCR Review\non:\n  pull_request:\n", encoding="utf-8"
    )
    (repo / BENCH_PATH).write_text("- id: fixture\n", encoding="utf-8")
    (repo / CONTROL_PATH).write_text("fn main() {}\n", encoding="utf-8")

    git(repo, "init", "-q", ".")
    git(repo, "add", "-A")
    git(repo, "commit", "-qm", "base")
    base = git(repo, "rev-parse", "HEAD")

    (repo / ".forgejo" / "workflows" / "review.yml").write_text(
        "name: OCR Review\non:\n  pull_request:\n  workflow_dispatch:\n", encoding="utf-8"
    )
    (repo / BENCH_PATH).write_text("- id: fixture\n  changed: true\n", encoding="utf-8")
    (repo / CONTROL_PATH).write_text("fn main() { let _ = 1; }\n", encoding="utf-8")

    git(repo, "add", "-A")
    git(repo, "commit", "-qm", "change")
    return base, git(repo, "rev-parse", "HEAD")


def flag_value(text: str, flag: str) -> str | None:
    """A flag's value from a shell run block: quoted (`--rule "x"`) or bare (`--effort low`)."""
    quoted = re.search(rf"--{flag}[=\s]+(['\"])(.*?)\1", text, re.S)
    if quoted:
        return quoted.group(2)
    bare = re.search(rf"--{flag}[=\s]+([^\s'\"\\;]+)", text)
    return bare.group(1) if bare else None


def workflow_invocation() -> tuple[str, str, str]:
    """The workflow's own ocr invocation: its --exclude, --effort and --timeout."""
    text = WORKFLOW.read_text(encoding="utf-8")
    values = {f: flag_value(text, f) for f in ("exclude", "effort", "timeout")}
    missing = [f for f in ("exclude", "effort", "timeout") if values.get(f) is None]
    if missing:
        raise SystemExit(
            f"FAIL  cannot read {', '.join('--' + m for m in missing)} from "
            f"{WORKFLOW.relative_to(ROOT)} — this check drives that invocation, so it "
            "cannot answer when it is unreadable"
        )
    return values["exclude"], values["effort"], values["timeout"]


def main() -> int:
    if shutil.which("ocr") is None:
        print("SKIP  ocr not installed; cannot reconcile the review scope")
        return 3
    try:
        rules = json.loads(RULE_FILE.read_text(encoding="utf-8"))["rules"]
    except Exception as exc:
        print(f"FAIL  cannot read {RULE_FILE.relative_to(ROOT)}: {exc}")
        return 1

    entry = next((r for r in rules if r.get("path") == ".forgejo/**"), None)
    if entry is None:
        print(
            "FAIL  review/rule.json has no '.forgejo/**' entry, so the trust-material "
            "checklist for workflow changes cannot reach the reviewer"
        )
        return 1

    try:
        exclude, effort, timeout = workflow_invocation()
    except SystemExit as exc:
        print(exc)
        return 1

    tmp = pathlib.Path(tempfile.mkdtemp(prefix="review-scope-"))
    server = None
    try:
        base, head = build_fixture(tmp)

        Stub.bodies = []
        server = http.server.HTTPServer(("127.0.0.1", 0), Stub)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        port = server.server_address[1]

        home = tmp / "home"
        home.mkdir()
        env = os.environ.copy()
        env.update(
            HOME=str(home),
            OCR_NO_UPDATE="1",
            OCR_LLM_URL=f"http://127.0.0.1:{port}/v1",
            OCR_LLM_PROTOCOL="openai",
            OCR_LLM_TOKEN="stub",
            OCR_LLM_MODEL="stub-model",
            OCR_LLM_AUTH_HEADER="x-api-key",
        )
        # review.yml's invocation, reproduced: same tool, same rule flag, same exclude —
        # only the refs and the destination file are local.
        proc = subprocess.run(
            [
                "ocr", "review",
                "--from", base,
                "--to", head,
                "--format", "json",
                "--output", str(tmp / "ocr-result.json"),
                "--rule", str(RULE_FILE),
                "--effort", effort,
                "--timeout", timeout,
                "--exclude", exclude,
            ],
            cwd=tmp / "fixture",
            env=env,
            capture_output=True,
            text=True,
            timeout=300,
        )
        bodies = list(Stub.bodies)
    except Exception as exc:
        print(f"FAIL  could not run the review scope reconciliation: {exc}")
        return 1
    finally:
        if server is not None:
            server.shutdown()
            server.server_close()
        shutil.rmtree(tmp, ignore_errors=True)

    if not bodies:
        # The review never reached the stub: that is "could not tell", never a pass.
        print(
            "UNVERIFIABLE  the reproduced review reached no provider request "
            f"(ocr exit {proc.returncode}); nothing about the scope was observed"
        )
        if proc.stderr.strip():
            print(f"  stderr: {proc.stderr.strip()[:200]}")
        return 3

    seen = "\n".join(bodies)
    reached_forgejo = FORGEJO_PATH in seen
    reached_bench = "bench/corpus/" in seen
    reached_control = CONTROL_PATH in seen
    reached_checklist = entry["rule"] in seen

    print(f"exclude from {WORKFLOW.relative_to(ROOT)}: '{exclude}'")
    print(f"provider requests: {len(bodies)}")
    print(f"  {'reached' if reached_forgejo else 'FILTERED OUT'}: {FORGEJO_PATH}")
    print(f"  {'reached' if reached_bench else 'excluded'}: bench/corpus/**")
    print(f"  {'reached' if reached_control else 'MISSING'}: {CONTROL_PATH} (control)")
    print(
        "  checklist text for '.forgejo/**': "
        f"{'resolved' if reached_checklist else 'NOT PRESENT'}"
    )

    failed = False
    if not reached_forgejo:
        print(
            f"FAIL  {FORGEJO_PATH} never reached the provider — a .forgejo/** checklist in "
            "review/rule.json is inert while review.yml excludes the path it covers"
        )
        failed = True
    elif not reached_checklist:
        print(
            "FAIL  the .forgejo path reached the provider without the '.forgejo/**' "
            "checklist text from review/rule.json"
        )
        failed = True
    if reached_bench:
        print("FAIL  a bench corpus path reached the provider; the bench exclusions are not optional")
        failed = True
    if not reached_control:
        print(
            f"FAIL  the control file {CONTROL_PATH} never reached the provider, so the "
            "selection is wrong for reasons other than the .forgejo exclusion"
        )
        failed = True
    if failed:
        return 1

    print(
        "review scope: .forgejo/workflows/review.yml is reviewed in the blocking path "
        "under the custom .forgejo/** checklist"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

# Releasing a prebuilt req

The owner's runbook for [ADR 0025](adr/0025-prebuilt-binary-per-pinned-rev.md): one release per pinned rev,
with a checksummed tarball per supported target. The adopter's side is `examples/install-req.sh` and
[`docs/adopting.md`](adopting.md) § *Install req*; this page is the side that cuts the release. Nothing here
changes req: the binary gains no command and no network client, and nothing is released until a person asks.

**Status.** `.forgejo/workflows/release.yml` and `.forgejo/workflows/install-timing.yml` were written
without a runner and have not been dispatched; the first dispatch of each is also its first test. Both fail
closed, so a misconfiguration is a red run, never a half-published release you cannot tell from a good one.

## When to cut a release

On request, for a rev an adopter is going to pin. Not on every merge: a release is a promise that its
checksums stay true, so there is no reason to make more of them than there are pins. The rev must be on
`main`. `release.yml` refuses a rev that is not an ancestor of `origin/main`, because the Baseline proof
inside the build script uses the trust anchor of the commit it builds (ADR 0010), and a commit off `main`
could carry its own key.

## The Linux path: dispatch `release.yml`

1. Settings, Actions, Secrets: `RELEASE_TOKEN` must exist (token scopes below).
2. Actions, *Release*, *Run workflow*, **from `main`**, input `rev` = the commit (a SHA or a ref on `main`).
   The workflow and `scripts/build-release.sh` come from the dispatched ref; the rev is built from a clean
   copy of itself.
3. The job checks the runner (C linker, `minisign`, `rustup` with the toolchain the rev pins), runs
   `scripts/build-release.sh --rev <full sha> --out …`, creates the release `rev-<REV>` if it is absent and
   uploads `req-<REV>-x86_64-unknown-linux-gnu.build-info`, `….tar.gz.sha256` and `….tar.gz`, in that order.
4. The last lines of the log are `released rev-<REV>` and the `pin (Linux): REV=… SHA256=… TARGET=…` line. Use
   that one: the build step's log also carries the script's own `pin:` line, printed before the upload, which
   after a half-failed upload names a hash that was never published.

The script refuses to write anything unless the binary says `built from` the bare rev, `req baseline
--verify baselines/air-gapped.json` prints `[VERIFIED]` and `req bench` passes its FMR gate (exit 1 names
the check; exit 3 is `minisign` missing, which is UNVERIFIABLE and blocks). A failed proof leaves nothing
published. If a run dies **during the upload**, before the tarball is there, nothing is pinnable yet: open
the release, delete the leftover `.build-info`/`.sha256` assets of that rev, and dispatch again. (They are
deleted by hand on purpose: the workflow has no way to delete or replace an asset.) A release whose tarball
is present is complete.

**The Linux floor is the runner's glibc.** `build-info` records the builder's libc line (`ldd --version`); a
host with an older glibc cannot run the tarball. Read that line in the `.build-info` asset before telling an
adopter a distribution is supported; a lower floor is a new ADR (musl was rejected in ADR 0025), not a flag.

## The macOS path: the same script, on the owner's machine

```bash
scripts/build-release.sh --rev <rev> --out "$(mktemp -d)"
```

It builds in a clean copy, proves the binary exactly as above, writes
`req-<REV>-aarch64-apple-darwin.{tar.gz,tar.gz.sha256,build-info}` into the directory and prints the `pin:`
line. Attach the three files to the release `rev-<REV>` with the Forgejo API, **refusing a name that already
exists**, tarball last. `RELEASE_TOKEN` here is a shell variable holding a `write:repository` token; the
header goes through stdin so the token is never in `ps`:

```bash
set -euo pipefail   # save this as a file and run it with `bash`: a failed upload must stop it, not scroll past
API=https://git.xantu-barb.ts.net/api/v1/repos/fdx/req
REV=<REV>; FULL=<full sha>; OUT=<the --out directory>; T=aarch64-apple-darwin
api() { printf 'Authorization: token %s\n' "$RELEASE_TOKEN" | curl --proto '=https' --tlsv1.2 -fsS -H @- "$@"; }

# The release for this rev: look it up, create it only if it is absent.
api "$API/releases/tags/rev-$REV" >/dev/null 2>&1 ||
  api -X POST -H 'Content-Type: application/json' "$API/releases" \
    -d "{\"tag_name\":\"rev-$REV\",\"target_commitish\":\"$FULL\",\"name\":\"rev-$REV\"}" >/dev/null
ID=$(api "$API/releases/tags/rev-$REV" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')

# Refuse a name that is already there, for all three, before uploading any.
api "$API/releases/$ID/assets" | python3 -c 'import json,sys; print("\n".join(a["name"] for a in json.load(sys.stdin)))' >"$OUT/.listed"
for n in req-$REV-$T.build-info req-$REV-$T.tar.gz.sha256 req-$REV-$T.tar.gz; do
  ! grep -Fxq -- "$n" "$OUT/.listed" || { echo "$n already exists: refusing to replace it" >&2; exit 1; }
done
for n in req-$REV-$T.build-info req-$REV-$T.tar.gz.sha256 req-$REV-$T.tar.gz; do
  api -X POST -F "attachment=@$OUT/$n" "$API/releases/$ID/assets?name=$n" >/dev/null || { echo "uploading $n failed: the release is incomplete" >&2; exit 1; }
  echo "uploaded $n"
done
```

If the Linux release for the same rev exists already, the release is simply reused, and the macOS names
differ from the Linux ones, so the two paths never contend for a name.

## What to pin in an adopter afterwards

The `pin:` line each path prints: `REV=<REV> SHA256=<hex> TARGET=<target>`. In the adopter's copy of
`examples/install-req.sh`, `REV=` takes the rev and `SHA256_<target with underscores>=` takes the hash for
that target. That first copy is trust on first use of the maintainer's release; from then on the pin in the
adopter's repository is the authority, and the install never reads the release's own `.sha256` asset. Run the
adopter's job once and check that `req --version` reports `built from <REV>`.

## What a release must never do

- **Replace, edit or delete an asset.** A checksum that has been pinned somewhere must stay true. A wrong
  release is superseded by a new rev, not repaired. (The workflow contains no PUT, PATCH or DELETE and
  `tests/release_workflow.rs` fails if one appears.)
- **Release a `-dirty`, `unknown` or `-cleanliness-unknown` binary,** or one that does not print `built from`
  followed by the rev it is named for. The script refuses; do not route around it with a hand-made tarball.
- **Release on every merge,** or from a trigger other than a person's dispatch.
- **Print the token.** No step traces (`set -x`), and a token goes to `curl` on stdin or not at all.
- **Fall back.** If a build, a proof or an upload fails, nothing else is substituted: the failure is the answer.

## Measured

Empty until the owner dispatches `.forgejo/workflows/install-timing.yml` (inputs `rev` and `sha256`, the two
values of a released rev's Linux `pin:` line) and the orchestrator copies the two `INSTALL` lines it prints,
and the run's URL, in here. No document gets a timing from anywhere else, and nothing is estimated.

## Tokens

| where | scope | for |
| --- | --- | --- |
| the adopter's CI secret `REQ_RELEASE_TOKEN` | `read:repository` | `examples/install-req.sh` downloading a release; also the timing workflow's secret of the same name |
| the repository secret `RELEASE_TOKEN` | `write:repository` | `release.yml` creating a release and uploading its assets; the owner provisions it |
| the owner's shell, for the macOS path | `write:repository` | the API calls above |

The read token replaces the SSH deploy key and the toolchain for *req*, not the need for a credential. Give
each its own token so either can be revoked alone.

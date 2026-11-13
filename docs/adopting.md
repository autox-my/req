# Adopting req

req is a requirements engine where a **claim** — one obligation, one file — is the unit, and a
claim's identity is a content address over its canonical meaning. This page is the whole
adoption path: install it, start a corpus, learn the two traps in the authoring loop, and gate
every variant in CI. An agent follows the same path: read it once, then use **For an agent** (near the end)
as the checklist — the five steps, what the agent may not do, what it can read, and the honest limit
of a test link. A command's exit codes live on its own `req <command> --help` page, not here.

The commands below were run in a scratch corpus: the install form at `2afbd68`, and `req init` with
a build of the revision that introduced it.

## Install req

req is deliberately not published to crates.io (`publish = false`), so **the rev is the provenance**: you pin a
commit, never a version number. Two ways honour that: a **prebuilt, checksummed binary** for the
rev (below; two releases exist so far) and a **build from source** at the rev ([From source](#from-source)).

> **Status: two releases exist.** `rev-ec87808` (Linux x86_64 only) and `rev-6b74f51` (Linux x86_64 and macOS arm64,
> Apple silicon only; no Intel Mac, no native Windows: see [Windows](#windows)). For any other rev or machine use
> [From source](#from-source). Both Linux tarballs were built on a runner whose libc is glibc 2.43 (each `.build-info`
> says so); read that line before you rely on one on an older distribution. The macOS tarball of `rev-6b74f51` was built on the
> owner's machine, not on a runner; from the next release the workflow builds it (ADR 0032, `docs/releasing.md`). `install-req.sh` was run against `rev-ec87808` on GitHub-hosted
> Ubuntu 22.04 (glibc 2.35), 24.04 and the latest image (2026-11-09): the right pin installed, `req --version` said
> `built from ec87808`, and `req init` and `req canon` exited 0; a wrong pin was refused with nothing installed (seen
> on 24.04 and latest). On 22.04 the binary ran but its loader printed ``weak version `GLIBC_2.39' not found`` on
> stderr. That run did not yet retry: a 504 from GitHub's download host (seen once on 22.04) was a refusal. The
> installer now retries a transient server error three times on the same URL; that change is tested against a local
> server, not yet against GitHub. It was run once against `rev-6b74f51`'s macOS tarball, on one macOS arm64 machine
> (macOS 27.2, 2026-11-09, anonymous download, a clean environment): the pin installed, and `req --version` said
> `built from 6b74f51` and `rules_v 7`. That rev's Linux tarball was checked against its `.sha256` but has not been
> installed through the script, and the macOS binary has not been run on a second Mac.

### Prebuilt

`examples/install-req.sh` (copy it out of req's checkout at the rev you pin) downloads the release
`rev-<REV>` for your machine, verifies the tarball against a SHA-256 *you* pin, and installs nothing
that is not that rev. It needs `curl`, `tar` and a SHA-256 tool (`sha256sum` or `shasum`); a release
is public (<https://github.com/autox-my/req/releases>) and needs no credential. The targets are `x86_64-unknown-linux-gnu` (Linux
x86_64; the release's `.build-info` states the glibc it was built against) and `aarch64-apple-darwin`
(Darwin arm64); any other machine is a non-zero exit that says so.

```bash
bash install-req.sh
```

(A private fork or mirror of the release sets `REQ_RELEASE_TOKEN=<a token that can read it>` and
`REQ_RELEASE_BASE`'s default host is its own to change; the `Authorization` header is sent **only** when
that variable is set and non-empty.)

Its first three lines are the **pin table**, the only thing you edit:

```bash
REV=<rev>
SHA256_x86_64_unknown_linux_gnu=<64 hex digits>
SHA256_aarch64_apple_darwin=<64 hex digits>
```

The file ships with **placeholders** in the table, and a placeholder, an empty pin or a malformed one
is a refusal before any request is made. Take the values from the release's `.sha256` assets the first
time — **trust on first use of the maintainer's release**, stated as such — and from then on the pin
in your repository is the authority: the install never reads the release's own checksum, because a
checksum fetched from the place that could be tampered with proves transfer, never authenticity.

After the download it verifies the tarball against the pin, checks the archive holds exactly one
member `req` before extracting anything, runs the binary, and requires `req --version` to exit 0
with `built from` equal to `REV`; only then does it install `req` (mode 0755) into
`${REQ_INSTALL_DIR:-$HOME/.local/bin}` and print the two lines that decide the binary, `rules_v` and
`built from`. Every refusal is a non-zero exit naming its cause — an unsupported platform, a missing
pin, a download that is not HTTP 2xx (curl first retries a transient 408, 429 or 5xx up to three times on the same
URL; an unknown rev is a 404, final; a wrong token on a private fork is a 401
or a 404), a hash that differs from the pin, an archive that is not exactly `req`, a
binary that does not run, a `built from` that is not `REV` — and there is **no automatic fallback**
to the source install: falling back on a mismatch would hide the very event the check exists to
surface. Choosing the source install is a separate act of yours.

A public release replaces the SSH key and the toolchain for *req* and asks for no credential. Only a
private fork sets a token: it travels in an `Authorization` header (read from stdin, never on a command
line, never printed); keep it in a CI secret, not in the repository. The public path was exercised against a
local server by `tests/install_req.rs` and against the published releases as the status note above says; the
private-fork token path has only run against the local server.

### From source

```bash
cargo install --git https://github.com/autox-my/req.git \
  --rev <PIN> --root "$HOME/.local" --locked req
```

The trailing `req` is the package: the repository holds three binary packages (`req`, `req-paraphrase`,
`req-tier2`), and without a name cargo refuses with *multiple packages with binaries found*.

`<PIN>` is the merge SHA of the req commit you depend on — read it with
`git rev-parse --short origin/main`; do not copy a SHA out of any document. The repository is public,
so no key and no credential is involved. The form is checked by its exit code (0) and by the installed
binary's `rules_v` and `built from` lines.

Two things make it a provenance, and each one is a failure without it:

| piece | what fails without it |
| --- | --- |
| `--rev <PIN>` | without `--rev` cargo builds the branch tip: there is no provenance to name |
| `--locked` | installs the committed `Cargo.lock`, which is part of the identity path |

### After either install

Put the directory the binary went to (`$HOME/.local/bin`, or `$REQ_INSTALL_DIR` if you set it) on `PATH` (a fresh shell does not have it, and without it the next
step's `req init` is `command not found`) and run `req --version` (exit 0). Two of its lines decide what
the binary is: **`rules_v`** makes a stored address current — an address is derived under a rules
version, so a verdict is not reproducible, and a Baseline not comparable, under another one — and
**`built from`**, the commit the binary came from: a bare SHA means *clean*, `-dirty` means tracked files
were modified, so the binary is not the commit it names (the output pads its columns, so a script that matches
the line needs one or more spaces between the words, not exactly one). This is the **only networked step**; everything
below runs offline against the corpus (REQ-0012).

Then run `req selftest` (exit 0, any directory, no corpus): known-answer checks compiled into the binary, which show that
*this build, on this machine,* derives what the project's reference build derives: a SHA-256 test vector, what a few statements
must derive from the hashing spec, pairs that must and must not share an address, the direction of hand-labelled changes, and the
seed parity corpus through the false-merge gate (ADR 0034). It prints the build it checked and `selftest: PASS (N checks)`; a `FAIL`
line says what was derived and what was expected, and the binary should not be used. It is not a defence against a tampered binary
(the pinned checksum is) and it does not show the rules are right; `info:` lines note whether `git` and `minisign`, which `req diff`,
`req pin` and `baseline --verify` need, are present.

### Air-gapped

Clone req once at the rev you want and install from the clone —
`cargo install --path . --locked`. req's core has no network dependency and providers are a local
process boundary (REQ-0012), so a corpus, a gate and a Baseline verification all work offline.

### Windows

There is no native Windows build: no tarball, no PowerShell installer, and the install script, the CI snippets and the
gate (`scripts/check.sh`) are `sh`/`bash`. On Windows, run req inside **WSL2** and install the Linux tarball there
(`install-req.sh` supports Linux x86_64). We have not run req on Windows, WSL2 included; the Linux binary is expected
to behave as on any glibc Linux, and that is an expectation, not a result.

Mind line endings in your repository. A Baseline is signed over the **exact bytes** of its manifest, and a committed
projection is compared byte for byte (`req render --check`). A Windows checkout with `core.autocrlf=true` rewrites text
files to CRLF, and a CRLF manifest or projection fails verification on a clean tree. Pin LF for them in your own
`.gitattributes`, whichever machine writes them:

```gitattributes
baselines/*.json     text eol=lf
baselines/*.minisig  text eol=lf
# each path listed in .req/projections, for example:
docs/spec.md         text eol=lf
```

(req's own repository pins LF per path for the files it compares byte for byte, not globally; the decision log's chain
and a claim's address are computed from parsed content, so a CRLF there is not a byte-for-byte hazard.)

## Start a corpus

```bash
cd myproject
req init
```

Exit 0, and exactly three files:

| path | what it is |
| --- | --- |
| `claims/REQ-0001.yml` | a starter claim in `req.claim/3`: its statement, provenance and `pins:` (the bound its statement must parse to) — nothing derived; the loader derives its parse and address every time it is read (ADR 0022) |
| `claims/README.md` | the authoring loop and its traps, in your project's tree |
| `aliases/subjects.yml` | the subject registry: what your prose calls a thing, and the id its address covers |

`init` writes into the **current directory** — never an ancestor — so a project can start inside
another one. It refuses (exit 1) if `claims/` or `aliases/subjects.yml` already exists — or if `aliases`
is a symlink, which would put the registry outside the tree you asked for — because the
alternative is overwriting your corpus. It takes no arguments and **no `--root`** (exit 2): a
silently ignored root would create a corpus somewhere you did not ask for, so run it where you want
the corpus. It prints the next steps, including the CI file. Confirm what you got:

```bash
req canon            # exit 0; the derived parse, and "pins: 3 on 1 claim(s), 0 flagged"
req gate             # exit 3 — the starter claim links no test yet (ADR 0019); see "Link each claim…"
```

If `req canon` is not exit 0, or flags a pin, the corpus is not yours yet — report it with the command and
the exit code (bottom of this page). `req gate` exits **3** here by design, not by accident: the
starter claim is machine-evidenced (the default) with no link and no report, an unexercised policy is
never a pass, and it has no process declaration to fall back on. The section *Link each claim to its proving test* is what turns it green.

## Adopting from documents you already have

A project usually starts with prose: a `spec.md`, a backlog, a capabilities table. `req init` gives
you a corpus, not those requirements. Since `req.claim/2` (ADR 0022) a claim's parse is **derived**
from its statement at load and nothing derived is stored, so what is honest to draft is exactly what
a human authors first: the statement, its provenance (the document **and the line**, recorded
relative to the corpus root), and empty `applicability` and `relations` — no slot to guess, nothing
to fill, and no pin.

req ships no importer and no `adopt` subcommand (issue #44): the mapping from a document to claims
is per-project. `tools/adopt.py` in req's repository is therefore a **starting point to copy and
adjust**, not an interface to depend on — copy it out of req's checkout at the rev you installed and
change it to fit your document.

```bash
# from the corpus root (the directory holding claims/), with adopt.py copied into your project
python3 tools/adopt.py --from docs/capabilities.md --date YYYY-MM-DD           # dry run
python3 tools/adopt.py --from docs/capabilities.md --date YYYY-MM-DD --write   # the drafts
req canon                # exit 0 — the parse each draft derives, structured or not
```

**The loop is the point.** A draft is a complete, legal claim the moment it is written, and it is
**still a proposal**: the human reads the parse `req canon` shows, re-words a statement whose parse
is not what was meant, adds the applicability and relations it needs, and pins a load-bearing bound
(`pins:`) where there is one (ADR 0004). The tool never writes a pin, never derives anything, and
never decides whether a bullet is one requirement or three. A structured `shall` and a prose-only
sentence are drafted alike — the difference is in the derived parse, which is req's to compute.

Two conventions are built in, and neither is a heuristic:

| `--convention` | what one claim is |
| --- | --- |
| `bullets` (default) | each markdown list item (`- `, `* `, `1. `) under `--under`, or in the whole document: the item's text, trimmed, its wrapped lines joined onto one line. A list item inside a fenced code block is code, never a candidate |
| `req-lines` | each line matching `- REQ-<n>: <text>`: the text after the colon. The source's id is **ignored** — req allocates `REQ-NNNN` — and the report says so |

`--under <heading>` limits the run to one section (a heading's subsections are under it, and a
heading inside a fenced code block is not a heading).
There is no `--subject` (exit 2 if given): the subject is derived from each statement against
`aliases/subjects.yml`, and an unregistered surface stays literal in the parse, so aliasing it is a
later, deliberate act. `--date` is the rationale date; without it the
source document's **last git commit date** is used, and the report names the commit it borrowed.
A document with **uncommitted changes** has no commit that describes its text, so that run is a
usage error asking for `--date`; with no commit at all it is the same. A date is authored, never
invented, exactly as `req baseline --at` is.

**Every skipped line is named with its reason and counted.** A line that looks like a candidate and
is not drafted — an empty item, a `req-lines` item with no id, an item outside the named heading,
or a repeat of an earlier item — appears in the report as
`skip <doc>#L<n> <reason>`, and the summary line says how many. A
requirement silently dropped at adoption is the failure this project exists to prevent, so the
report is the reviewable output.

The defaults are the safe direction: a **dry run** prints each draft, the file it would write
(`claims/REQ-NNNN.yml`, the next free number) and the skips, and writes nothing; `--write` writes
them. It never overwrites — an existing path is exit **1**, nothing written — and a document already
drafted into the corpus is refused (exit **1**) with the claims it produced named: draft that
document again only after deliberately deleting them, or copy the script and adjust it. Only a
source carrying the extractor's own `<doc>#L<n>` shape counts as an earlier draft — `req init`'s
starter claim names `docs/requirements.md` as a pencil sketch, and that does not block adopting
from it. Exit **0** drafted (dry or written), **1** a definite problem, **2** usage (including a
date the tool will not borrow), **3** the document, the corpus, the subject registry or `req`
cannot be read — no `claims/` means run `req init` first.

### From a Sphinx-Needs `needs.json`

If the requirements already live in a Sphinx-Needs project, its export is the source and nothing needs
retyping. `tools/needs_import.py` (a sibling of `adopt.py`, python3 standard library only, also a
**starting point to copy and adjust**, not a `req` command; ADR 0027) reads the export and writes the same
legal drafts: the statement, its provenance, empty `applicability` and `relations`, nothing derived, no pin.

```bash
# from the corpus root, with needs_import.py copied into your project
python3 tools/needs_import.py --from needs.json --origin <where the export came from> \
    --type <your requirement type> --type <another> --date YYYY-MM-DD           # dry run
python3 tools/needs_import.py ...the same arguments... --write                  # the drafts
req canon                # exit 0 — the parse each draft derives
```

**Name your types.** `--type` is required and repeatable, and there is no default list: the type names
are your project's own (one export's `feat_req` is another's `sysreq`), and an export also holds
documents, architecture and test runs that are not requirements. A need of a type you did not name is
not ignored — the report counts it, by type. A `--type` the file does not have is exit **1** and the
message lists the types it does have, so a typo cannot select nothing quietly. `--status` (repeatable)
narrows the selection further; the rest count as not selected.

**The statement is the need's `content` when that is one plain paragraph** — its lines trimmed and joined
with one space. Anything else is skipped and named, by the first reason that applies:

| reason | when |
| --- | --- |
| `external` | `is_external` is true: the export does not own the need |
| `no content` | `content` is absent, null or blank |
| `rst directive` | a line of the content starts with `..` |
| `multi-paragraph` | a blank line inside the content: whether the second paragraph is part of the obligation or a note is a person's decision |
| `rst role` | an inline `:role:` followed by a backtick |

The reader does not strip markup and does not cut a need down to its first paragraph; a skipped need is
a need for a person to look at, not one the tool rewrote.

**The need's id is the provenance and req allocates the claim id.** Each draft's `rationale.source` is
`<origin>#<need id>` (`--origin` is the URL or path the export was retrieved from; the document name and
line go in the draft's header comment). A need already drafted from the same origin is refused (exit
**1**, nothing written), so re-running does not duplicate; the same file under a different `--origin` is
a different provenance. Two needs with the same text — the same sentence at two levels — are two drafts,
and `req render` shows them with one address: that is for you to read, not for the tool to merge.

**Not carried, and the report says so.** The status, tags, safety and security levels, `reqtype` and every
other field are not written (a source status is not a ratification; applicability is authored), and
neither are the typed links: `relations` stays `[]`, because a relation names a claim id that does not
exist yet and the kind of relation is a judgement. The `not carried` line lists the fields it saw on the
drafted needs and counts the drafted needs that had typed links, naming the fields. The link types are the
ones the export declares (`needs_schema`, a field whose `field_type` is `links` or `backlinks`), and the line
says "declared by needs_schema"; an export without one (an older Sphinx-Needs) is read by inference (a list of
ids that is `links`, ends in `_back`, or has a `<name>_back` partner field in the file) and the line says
"inferred". Every other field is listed too, so nothing is hidden. `--areas` carries one thing more, the place: a
need's `docname`, split on `/`, each part made a key (lowercase it, replace each run of characters outside `[a-z0-9]` with `-`, strip `-` from both ends), joined by `/`, becomes the
draft's `area` (ADR 0051). The draft is then `req.claim/3`, with `area:` after `claim_id`, and needs a `req` that
reports `claim_schema req.claim/3` (exit **3** otherwise); a need with no `docname` stays `req.claim/2` with none.
A section title or number is never a key, and the statement is exactly what it is without the flag. A part that
leaves no key, or two names that make one path, is exit **1** naming them and writes nothing. `--area-depth N`
keeps the outermost N parts. The reader proposes the matching `.req/areas` lines (`<path> <label>`, the label the
docname as written, cut to that depth): a dry run prints them, `--write` appends those the file does not declare
and leaves a declared path alone whatever its label, and the report adds the `areas:`, `no area:` and `registry:`
lines. The registry is yours to edit afterwards.

**The report adds up.** It ends with one line accounting for every need in the file, counted from the
`needs` mapping itself (an export's own `needs_amount` is never trusted):

```text
needs N = drafted D + skipped S + not selected M (by type: ...)
```

The drafts are proposals (ADR 0004), exactly like the ones `adopt.py` writes: read the parse `req canon`
shows, re-word what does not parse as meant, and pin a load-bearing bound. `--date` is required (the
retrieval date of the export; an export has no trustworthy one, and the tool never reads the clock). A dry run is the
default. Exit **0** drafted, **1** a definite problem (nothing drafted, an unknown type, already drafted),
**2** usage, **3** the export, the corpus or `req` cannot be read honestly (not JSON, no `versions`, several
versions and no `current_version` — pass `--version-key` — or a corpus the loader rejects).

### From a ReqIF file (DOORS, Polarion, …)

DOORS, Polarion, Codebeamer, Jama and Eclipse ProR export ReqIF, and an OEM often sends a supplier one.
`tools/reqif_import.py` (a sibling of `needs_import.py`, python3 standard library only, also a **starting
point to copy and adjust**, not a `req` command; ADR 0028) reads a `.reqif` file and writes the same legal
drafts: the statement, its provenance, empty `applicability` and `relations`, nothing derived, no pin.
**The yield on a real specification is unmeasured**: the reader was built on three small public files, so
what it drafts and what it names as skipped on yours is the first measurement, not a known rate. A `.reqifz`
is a zip: unzip it and pass the `.reqif` inside (the script opens no archive). A PDF is the rendering of data
ReqIF holds with its structure; read the ReqIF, not the PDF.

```bash
# from the corpus root, with reqif_import.py copied into your project
python3 tools/reqif_import.py --from module.reqif --list                    # what is in the file; writes nothing
python3 tools/reqif_import.py --from module.reqif --origin <where it came from> --date YYYY-MM-DD \
    --type <an object type> --text-attr <the attribute holding the statement> \
    --id-attr <your own id attribute>                                       # dry run
python3 tools/reqif_import.py ...the same arguments... --write              # the drafts
req canon                # exit 0 — the parse each draft derives
```

**Run `--list` first.** It prints the header, each specification with its object count, and each object type
with its attributes (declared kind, and how many objects carry a value). A DOORS object type has no name at
all: it shows as `(unnamed)`, which is display only and never a selector, and you select it by its
`IDENTIFIER`. The attribute that holds the requirement text differs by tool and by module (`ReqIF.Text` is
common, but a DOORS module can rename its columns and an Eclipse ProR export may have none), so there is no
default: `--text-attr` is required, and a text attribute none of the selected types defines is exit **1**.

**Name your selection.** At least one of `--type` (an object type's name, else its `IDENTIFIER`; repeatable)
and `--where NAME=VALUE` (repeatable, all must hold; an enumeration such as a status compares its names, and a
value of any other kind compares its text) is required: a built-in selection would be a guess about your
vocabulary. An object you did not select is not ignored; the report counts it by type.

**The statement is the text attribute when that is one plain paragraph.** The value's own kind decides how
it is read (a plain string and an XHTML value can sit under the same attribute). Anything else is skipped
and named, by the first reason that applies:

| reason | when |
| --- | --- |
| `no text` | the attribute has no value for the object, or a blank one |
| `multi-paragraph` | a blank line in a plain string, or more than one block element in XHTML |
| `rich markup (<element>)` | any element inside the one `div`/`p` wrapper, or a wrapper that is not one; the first is named (`br`, `span`, `sub`, `ul`, `table`, `object`, …) |
| `not text (<KIND>)` | the text attribute's value for this object is an integer, a date, an enumeration or another kind with no text to take (it is named, never turned into text) |

Nothing is flattened to make an object fit: a subscript taken out of `m<sub>2</sub>` is a changed meaning, which
is the failure this project exists to prevent. If your export is mostly `rich markup`, that is a finding to
report, not something this script loosens.

**The object's `IDENTIFIER` is the provenance, and req allocates the claim id.** Each draft's
`rationale.source` is `<origin>#<IDENTIFIER>` (`--origin` is the URL or path the file came from). `--id-attr`
names the attribute holding your own human id; it is shown in the report and in the draft's header comment,
never as the claim id. An object already drafted from the same origin is refused (exit **1**, nothing
written). Whether an `IDENTIFIER` survives a second export of the same module is not verified: some tools
regenerate identifiers per export, so there is no re-sync from a later export yet.

**Not carried, and the report says so.** Every attribute other than the text attribute (the `not carried`
line counts the drafted objects carrying each, so a status or object type you did not select on is visible),
the specification hierarchy (a `hierarchy` line counts the drafted objects that sit under a parent), and the
relations (a `relations` line counts the file's relations, the drafted objects they touch and their types;
`relations` stays `[]` because a relation names a claim id that does not exist yet and its kind is a
judgement). The object `IDENTIFIER`s are not claim ids. `--areas` carries the hierarchy as the place (ADR 0051): a
drafted object's `area` is the keys of its ancestors, outermost first, joined by `/`, each key made from the ancestor's `IDENTIFIER` (lowercase it, replace each run
of characters outside `[a-z0-9]` with `-`, strip `-` from both ends: `_Sec.A 12` is `sec-a-12`). A heading's text
or a section number is never a key, the specification is not a segment, and a drafted parent is a segment of its
children's paths. An object with no ancestor, or one the hierarchy reaches more than once, is drafted with no
area (the report counts each by reason); an `IDENTIFIER` that leaves no key, or two that make one path, is exit
**1** naming them. A draft with an area is `req.claim/3` and needs a `req` reporting `claim_schema req.claim/3`
(exit **3** otherwise). `--area-depth N` keeps the outermost N segments, and `--area-label-attr NAME` names the
attribute holding a heading's text, the label of its path (else the `--id-attr` value, else the last segment).
The reader proposes the `.req/areas` lines; `--write` appends the ones the file does not declare, leaves a
declared path alone, and puts the file back if the claims then fail to write.

**The report adds up.** It ends with one line accounting for every object in the file, counted from the file
itself:

```text
objects N = drafted D + skipped S + not selected M (by type: ...)
```

The drafts are proposals (ADR 0004): read the parse `req canon` shows, re-word what does not parse as meant,
and pin a load-bearing bound. `--date` is required (the retrieval date of the file; the header's creation
time is shown and never used). A dry run is the default. Exit **0** drafted (or `--list`), **1** a definite
problem (nothing selected or extracted, a type the file does not have, `--text-attr` or `--id-attr` defined on
none of the selected objects' types, already drafted), **2** usage, **3**
the file, the corpus or `req` cannot be read honestly (not XML, a DOCTYPE or entity declaration, a missing
or repeated object `IDENTIFIER`, a reference that does not resolve, an attribute name matching two
definitions, a corpus the loader rejects).

## The document your project can commit

A `spec.md` is usually a second source of truth: it drifts, and a "generated, do not edit" comment
with nothing behind it proves nothing. req makes the document a **projection** instead (ADR 0018) —
generated from the corpus, committed as an artefact, and proven current by regeneration:

```bash
req render --check docs/spec.md   # the gate: 0 current, 1 stale or hand-edited, 3 cannot judge
req render --out docs/spec.md     # the remedy: regenerate, then commit the result
```

`--check` regenerates the projection and compares **bytes**; it writes nothing. A **1** means the
corpus moved or the file was edited — the remedy is the same either way: change the corpus or
regenerate, never hand-edit. A **3** means it cannot judge: the file is not a projection (no
header), another build wrote it (`rules_v` differs, so its addresses mean something else — the
same rule as a Baseline's `rules_v` mismatch), its selection cannot be reproduced, or the corpus
cannot be read. Running `--out` first and then `--check` proves generation is reproducible, but the
**gate is `--check` alone**: `--out` before it would repair the staleness it is supposed to report.

The first line of a committed projection is a machine-readable header naming the `generator`,
`rules_v`, selection, applying-claim count and an `sha256:` identity over the sorted applying
addresses, so a reader can see what it was built from before running anything. It is not the check
— the corpus is the oracle, and the header alone agreeing while the body differs is exactly the
failure case `--check` exists to catch.

Pin the paths your project commits in **`.req/projections`** — one repo-relative path per line,
`#` comments, blank lines skipped, the same grammar as `.req/variants` — and `req check` (see *CI*)
runs `req render --check` for each (or `req trace --check`, for a traceability matrix written with `req trace --out`) in the same step that gates your releases. A listed path that is
missing is a failure (a projection that vanished is silent decay), and a list that checked nothing is
not a pass. The list
is deliberately small: a projection is committed where it is a **deliverable**, not as a cache of
every view, and adding a path is a decision. A `spec.md` cannot go stale by construction, because
the only way to change it is to change the corpus — which has its own gates (`req gate`,
`req baseline --verify`).

A graph page for a reader who will not read a matrix is rendered, not committed: `req trace --format json` carries every
claim as a node and every relation as an edge, and `examples/graph/README.md` turns that document into one standalone
page with a deterministic converter and a pinned, offline renderer. The page is as current as the run that made it.

## The authoring loop and its traps

Every command's `--help` carries its own `exit:` line — the codes that command answers,
including the tri-state — and `req <command> --help` works before a corpus exists.

1. **Edit `statement`, then run `req canon` to see the parse.** Nothing derived is stored in a
   claim file (`req.claim/3`, ADR 0022): the parse, the canonical form and the address are computed
   every time the claim is loaded, so there is nothing to fill and nothing to hand-write. `req canon`
   prints, per claim, the derived structure, modality, subject, operator, value, unit and lints, and
   each pin's verdict.
2. **Pin what is load-bearing.** `pins:` (any of `modality`, `subject`, `operator`, `value`,
   `unit`) states what the statement must parse to. A pin agrees, or it is a flag — a
   *contradiction* (the parse says something else) or *not carried* (the parse holds no such
   value); there is no third category. A flagged pin moves no address; `req gate` fails on it
   (`pins_hold`, exit 1). A pin is **not attested by the Baseline**: deleting one after signing
   still verifies, so a pin is a check at gate time, not a signed promise.
3. **Values are canonical SI.** `within 200 ms` parses to `value "0.2"`, `unit s`: pin the exact
   decimal string the parser derives, not the units the prose used. `unit: ms` is a contradiction,
   and `"0.30"` is not a canonical decimal (`"0.3"` is), so it cannot be pinned.
4. **`req canon` fails on a flag only if you ask.** It reports (it exits 3 only when the corpus cannot be read); `req canon --fail-on-flag` is the opt-in that exits 1 on a flagged pin, for the time before every claim has a linked test. The check that gates a release is `req gate`:
   the loop in one sentence is edit → `req canon` to see the parse → `req gate`. A claim's
   `applicability:` list is part of its address (since rules v5), so editing it moves the address
   exactly as editing the statement does.
5. **An empty `claims/` is UNVERIFIABLE (exit 3)** — for `canon` and `gate` alike. Zero claims
   cannot be told from every claim having vanished, so never "clean up" a corpus down to nothing;
   delete a requirement by deleting its claim, deliberately.
6. **A `req.claim/1` file is NOT JUDGED (exit 3).** Migrate it with `tools/migrate_claims.py` from
   req's repository — a dry run by default that prints every key it drops and every pin it creates;
   `--write` rewrites the file. A format bump moves no address. A `req.claim/2` file needs no migration:
   a `req.claim/3` build loads it as it is.
7. **Several authors, one id space: `req next-id`.** `req next-id --count 5` prints the next five free ids
   (highest existing number + 1; a gap is never refilled, and it reads file names, so it answers while a
   sibling claim is half-written). To hand a block to a branch, `req next-id --count 5 --reserve` also appends
   it to `.req/reserved` (`REQ-0143..REQ-0147`, committed with the branch): later calls, on any branch that has
   merged it, start after the block. Two branches that reserve at the same time edit the end of one file and
   git reports a merge conflict: that is the signal, there is no lock. See `req next-id --help`.
8. **File a claim where your specification files it: `area`.** A claim may carry one `area: <path>`
   (`req.claim/3`; for example `area: diagnostics/startup`), the place a reader would look for it. Each
   segment is a short stable key (`[a-z0-9][a-z0-9-]*`), not a heading and not a section number, which move
   when a section is inserted; the name a reader sees, in any language, is the label on that path's line
   in `.req/areas` (`<path> <label>`). Filing is placement only: it enters no address and no Baseline, so
   re-filing a claim needs no re-signature, and a statement still has to stand alone without its heading. A path
   `.req/areas` does not declare is listed by `req check` as `undeclared_area` and fails only if
   `.req/completeness` says `strict areas`. The format is in `docs/claim-format.md`, *Area*.

## Sign the Baseline

`req baseline --at <ts>` prints the canonical manifest (envelope v4, ADRs 0016 and 0019) — the exact
bytes a signature covers — and **req never signs**: the signature is the human's act, and a key the tool could
reach would prove nothing (ADR 0010). A manifest with no valid signature over it is *not yet a Baseline*,
and verification says so.

Use `--out <file>`, not a shell redirect (`> file`): the shell truncates the target before `req` runs, so a
refusal (an older `req`, an absent decision log) leaves an empty file that `minisign -Sm` will sign without complaint.
`--out` writes a temp file and renames it, so a refusal leaves whatever was there, and exits non-zero.

```bash
req baseline --at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --out baselines/release.json
                                                # the timestamp is YOUR assertion; the file is written only
                                                # once the manifest is complete
minisign -Sm baselines/release.json             # signs it; prompts for the secret key's passphrase
req baseline --verify baselines/release.json    # [VERIFIED] once signed, "not yet a Baseline" before
```

- **`--at` is required and is the signing time.** It goes *inside* the signed bytes, so it cannot be
  backdated without breaking the signature. Re-emit with the current time just before you run
  `minisign -Sm`: copying an older manifest's time into a re-sign backdates it, and a key revoked in
  that gap would still verify. Req never invents one: emitting without `--at` is a usage
  error (exit 2). A v3 manifest also attests the Decision log's chain head, and emission refuses (exit 3)
  when that log is absent, unparsable or tampered — a head that attests nothing must not be signed.
- **One Baseline per release line.** A manifest covers the claims its selection makes apply, so pass a
  release line's words to cover that release: `req baseline --at <ts> --out baselines/<release>.json
  <the line's key=value words>`. A release no Baseline covers is `req check`'s `in_no_baseline` (exit 3).
- **`-Sm` is `-S` (sign) with `-m <manifest>`**, and its default output name is `<manifest>.minisig` —
  exactly the file `--verify` looks for. Name it otherwise with `-x <path>` and pass `--signature <path>`.
- **Sign the bytes `req baseline` emitted.** Do not reformat the file: the signature covers those bytes,
  and verification compares them to the corpus as well.
- **The secret key is yours and is not in the repository.** minisign reads `~/.minisign/minisign.key` by
  default, or `-s <secret key>`; `req` has no flag for it and needs none.
- **`signed_at` judges the key, never the claims.** A key whose validity ends **at or after** the signing
  time verifies for that Baseline even when today is later; a key that had already **expired** at the
  signing time fails (it was not valid when the signature was made), and a key **revoked after** the
  signing time no longer invalidates it — revocation is prospective. Whether a claim is *current* remains
  `baseline --verify`'s and `audit`'s business, which re-derive addresses. `--now <date>` is the explicit
  override for the judgement time; it is a verification flag, not an emission one.
- **`baselines/trust.yml` must carry the matching public key**, or verification is UNVERIFIABLE — an
  unknown key is never a pass. Rotate by adding a key beside the old one with its own validity window
  (`valid_from`, `valid_until`, `revoked_at`): old Baselines stay verifiable against the old key, and a
  key revoked at or before a Baseline's signing time FAILS.
- **An edit after signing is FAIL, not drift**: verification re-derives every address, so a claim edited
  once the manifest was signed is the same class as a hand-edited digest. A v3-or-later manifest also
  FAILs when the Decision log no longer re-derives the chain head it records; an absent log is
  UNVERIFIABLE, never FAIL.
- **v4 attests the evidence declaration and its links; v3 does not.** A v4 manifest records each
  claim's `tests_hash`, so a `tests:` or `evidence:` edit after signing is **FAIL** — the same class as
  an edited address. A v3 manifest still
  verifies on its historical signed bytes and says **once** that it predates v4: its tests are *not
  attested*, which is not a failure, and a release that wants attestation re-emits and re-signs as v4.
- **A `rules_v` mismatch is UNVERIFIABLE** with the re-sign remedy: an address is derived from the rules
  version, so the comparison is void rather than failed. A v2 manifest still parses and is reported as
  *predates … re-issue and re-sign* — re-emit with `--at` and sign again.

## Signing by several parties

A release two organisations must both accept (an OEM and its Tier-1) is signed by each, in a named
capacity, over **the same manifest bytes** (ADR 0042). Nothing about emission changes; each party adds
one file beside the manifest, so a countersignature never invalidates the signature already there.

1. **Give each key a party and a role** in `baselines/trust.yml`. The role is the *meaning* of every
   signature the key makes; someone acting in two capacities holds two keys.

   ```yaml
   keys:
     - id: "oem-ro"
       scheme: "minisign"
       public_key: |
         untrusted comment: minisign public key …
         RWQ…
       valid_from: "2026-11-01"
       party: "oem"
       role: "requirements-owner"
   ```

2. **Say who must sign** in `.req/signing`, one `<party> <role>` per line (`#` comments and blank lines
   are skipped). A line repeated *n* times needs *n* signatures from *n* distinct keys: one key counts
   once.

   ```text
   oem requirements-owner
   acme-tier1 safety-manager
   ```

3. **Each party signs the manifest** into `<manifest>.<key-id>.minisig` and commits the file:

   ```bash
   minisign -S -m baselines/release.json -s oem-ro.key -x baselines/release.json.oem-ro.minisig
   ```

   `baselines/release.json.minisig` still works and is read as a signature by the `--key` key (default
   `req-baseline-1`), so a repository that never uses this section is untouched: no `.req/signing`
   and no keyed file verifies exactly as before.

`req baseline --verify` judges every signature present, then the policy, and its header names each
signer (`key oem-ro (oem requirements-owner)`). **Awaiting a party is not a broken signature:**

| verdict | reads as | what to do |
| --- | --- | --- |
| `[UNVERIFIABLE]` `awaiting acme-tier1 safety-manager` | the others verified; nobody holding that party and role has signed | get that party's `.minisig` committed |
| `[UNVERIFIABLE]` `key X is not in the trust file` | a signature file names a key nobody trusted | add the key by pull request, or delete the stray file |
| `[FAIL]` `key X: the signature does not verify` | the bytes were edited, or the wrong key signed | re-emit and re-sign; never edit a signature |
| `[FAIL]` `key X is revoked …` | the key was revoked at or before the signing time | sign with a current key |

`req check` keeps this step blocking: a release awaiting a party is not ready to ship. `req check`'s
completeness step counts a manifest as signed when *any* signature file lies beside it.

**Exchange across organisations is offline.** The supplier pins the OEM's corpus at a commit
([a corpus kept in another repository](#a-corpus-kept-in-another-repository)) and commits the OEM's
public key to its own `trust.yml` with `party: oem`; the OEM's Baseline and signatures then verify in
the supplier's checkout with no network. The supplier's countersignature travels back as one file.
Who may read or push stays with your git host.

**A signed Decision.** `decisions/DEC-NNNN.yml.<key-id>.minisig` is a signature over the record's
canonical bytes. `req decision --verify` then reports `signed by <id> (<party> <role>)` and requires
the record's `role` to equal the key's; a mismatch is FAIL. A record with no signature file stays
valid and is listed as *asserted*: its role is a claim, not a proof.

## Link each claim to its proving test

A claim may carry `tests:` — the test ids that prove it. The list is **authored**, like its rationale,
and it is a claim about evidence: link a claim only to the test that demonstrably proves it. **A wrong
link is worse than no link** — no link is UNVERIFIABLE and says so, while a wrong link manufactures
confidence.

```yaml
tests:
  - "req::canon::a_rewording_keeps_its_address"
```

A test id is `classname::name` — the two attributes the report's `<testcase>` carries, joined by `::`
— so the `classname` may itself contain `::`. Read the id out of the report rather than retyping it:
an id compared exactly means a typo is "linked test not in the report", not a silent match.

- **It is outside identity.** Adding, removing or renaming a link moves no address — a test rename must
  never move a requirement. What moves instead is `tests_hash`, which the signed Baseline attests, so
  unlinking — or flipping the evidence mode — after signing is FAIL rather than a silent edit.
- **It is optional.** A **machine** claim with no `tests:` (or an empty list) simply links nothing; the
  gate names it, per claim. That is the flagship answer, not a parse error.
- **Leave unlinked what you cannot prove.** An unlinked claim is exit 3, never a PASS; an agent cannot
  make it go away without adding a link that survives review.

**Process evidence is a declaration, never a silence.** If the process itself establishes the claim — a
recorded Decision, a signature, a workflow — say so, and link nothing:

```yaml
evidence: process
tests: []
```

The gate reports it **PASS as declared**, names it (`* process-evidenced (declared, not machine-proven)`)
and counts it; it is never folded into the machine pass. A process claim must carry no links (naming both
is a load error, exit 3), and an empty `tests:` list on a machine claim is *no linked test* —
UNVERIFIABLE, never process by accident.

The report comes from **nextest** (ADR 0019) — adopted for the JUnit artefact, not for speed: measured
2026-10-21, a warm `cargo test` took 2 s and `cargo nextest run` 2–3 s, both paying the ~24 s link over
the same tests.

```bash
cargo install cargo-nextest --version 0.9.149 --locked   # pinned; verified at pin time
cargo nextest run --profile ci                            # writes target/nextest/ci/junit.xml
req gate --test-report target/nextest/ci/junit.xml deployment_mode=air_gapped
```

The profile is reviewed, not improvised (`.config/nextest.toml`):

```toml
[profile.ci]
fail-fast = false

[profile.ci.junit]
path = "junit.xml"
report-skipped = "ignored"
```

`report-skipped` belongs **under `[profile.ci.junit]`** — the top-level spelling is silently ignored in
0.9.149 and would drop every `#[ignore]`d test from the artefact — and its value is `"ignored"`, not
`"all"`, which would also list tests skipped by filters or partitions. `cargo test` is **replaced** by
this run, not kept beside it: one run, one truth. One gap, named rather than hidden: nextest does not
run doctests, so a `#[doc]` example needs its own step.

**pytest writes the report itself.** `pytest --junitxml=target/pytest/junit.xml` is read as it is
(ADR 0024; tested against real pytest 9.2.0 output, `tests/fixtures/junit/`). The id is the report's
`classname::name`: `tests.test_probe::test_passes`, `tests.test_probe::test_param[a b]` (a parametrized
id keeps its brackets and spaces), `tests.test_probe.TestGroup::test_in_class` (a class method). Copy the
id out of the report; do not retype it. Outcomes map as pytest means them: **passed** is passed;
**failed**, a strict XPASS and a setup error are failed; `skip` and `xfail` are **skipped**, and a
skipped test is not a pass. One shape is refused whole: a **collection error** (an import that fails
while pytest gathers tests) is written with an empty `classname` and interrupts the run, so nothing
else in that report is evidence — the message says so; fix the collection and rerun.

**Swift Testing writes its own file.** `swift test --xunit-output <dir>/junit.xml` puts Swift Testing's
results in the sibling `<dir>/junit-swift-testing.xml`; with only Swift Testing in the package the
requested `junit.xml` is never created, so read the file that appeared rather than the one that was
asked for. The parent directory must already exist — SwiftPM does not make it. The id is again
`classname::name`, and both halves carry what a machine needs: `classname` is the suite path
(`DiskTriageCLITests.ImagingMapfileTests`) and `name` keeps its trailing `()`, with req decoding XML entities so a `tests:` entry holds the decoded name
(`DiskTriageCLITests.ImagingMapfileTests::REQ0108_partialMapfileClipsTheUnboundedTail()`), so a claim id
written into the function name is enough to generate the link — copy the id out of the report all the
same. **A parameterised test is one case**: `@Test(arguments: [1, 2]) func f(_ v: Int)` yields a single
`testcase` named `f(_:)`, with no per-argument case as pytest's `f[1]` and `f[2]`, so a failure in any
argument fails the whole case and no single argument can be linked on its own. A skipped case is a
`<skipped/>` element, which is not a pass, as elsewhere. A filtered or empty run exits 0 with a report
holding **zero test cases**, which is UNVERIFIABLE — the answer the policy already gives, not a hole.

Measured on one adopter package (disktriage, Swift 6.4, Swift Testing only, macOS):
`req gate --test-report` consumed the report **with no change to req**. That is one shape from one
package, not a promise for every Swift project; other producers are unpromised until a real report from
them has been tested.

**A command is a test case.** A shell check, a self-test or a script that exits 0 / 1 / 3 has no report
of its own; `tools/junit_case.py` (stdlib Python 3, copy and adjust like `tools/adopt.py`; req still
runs nothing) runs it and records one `<testcase>`:

```bash
# start a run: the first case begins a new report so no earlier row can answer for this one
python3 tools/junit_case.py --fresh --report target/checks/junit.xml --classname checks --name health \
  -- sh scripts/check-health.sh
python3 tools/junit_case.py --report target/checks/junit.xml --classname checks --name audit \
  --timeout 300 -- python3 scripts/audit_selftest.py
req gate --test-report target/checks/junit.xml deployment_mode=air_gapped
```

with `tests: ["checks::health"]` in the claim it proves. The adapter exits with the command's own
status, so wrapping a check never hides its failure, and it leaves the command's output alone. Exit 0
is passed, **1 a failure**, **3 skipped** (`UNVERIFIABLE (exit 3)` in the case's message) and anything
else — another status, a signal, a missing executable, a timeout — an error; re-running a case replaces
its row, and a report it cannot parse is refused, never overwritten (`--fresh` starts over). The
consequence of 3 → skipped is stated rather than hidden: a claim whose linked tests are all skipped is
**FAIL, exit 1** in `tests_linked` (the table below), never a pass and not exit 3 — a check that says
"cannot confirm" is a red the claim's owner must look at. Linking a claim to a wrapped check is a claim
about evidence, reviewed like any link: it proves only what the script checks. `python3
tools/junit_case.py --help` is the contract.

**Several reports.** `--test-report` may be repeated, in `req gate` and in `req check` alike, and the
gate judges the **union**:

```bash
req check --test-report target/nextest/ci/junit.xml --test-report target/pytest/junit.xml \
          --test-report target/checks/junit.xml
```

A test id in two reports with the same outcome counts once; with different outcomes the evidence is
unusable and the message names the id and both files. A report that cannot be read or parsed makes
`tests_linked` UNVERIFIABLE **once**, naming that file — a test absent from the union might be in the
one that failed to load, so the other reports do not get to answer for it. A union with no test case
is "carries no test cases", as for one report. Ids from different stacks do not collide because their
`classname`s differ. Each report is unsigned and the run unattested (ADR 0019 §8), as before.

**`cargo test` is a named gap.** libtest writes no report and its JSON format is unstable. Two
routes are supported: adopt nextest (above), or wrap the whole suite as **one** case,
`python3 tools/junit_case.py --report target/checks/junit.xml --classname suite --name cargo_test --
cargo test`, which links a claim to "the whole suite passed" and says nothing finer.

The `tests_linked` policy answers four ways over the applying claims, and there is no fifth:

| outcome | condition | fold |
|---|---|---|
| **PASS** | the claim links at least one test and at least one linked test **passed** | contributes pass |
| **FAIL** | every linked test is present in the report and **none passed** (a `skipped` test is not a pass) | exit 1 |
| **UNVERIFIABLE — *no linked test*** | the claim carries no `tests:` entry — the flagship answer, named per claim | exit 3 |
| **UNVERIFIABLE — *linked test not in the report*** | a linked test id has no testcase in the report (renamed or deleted) — never folded into PASS | exit 3 |

The four outcomes judge **machine** evidence: a claim declaring `evidence: process` is answered by its
declaration, which is a reported fact, not a fifth outcome.

An absent, unparsable or zero-testcase report is UNVERIFIABLE **once**, naming the reader's reason —
never once per claim. `FAIL` outranks any UNVERIFIABLE; both outrank PASS, and the exit code is the
existing fold. **A gate with no `--test-report` cannot exercise the policy and is UNVERIFIABLE (exit
3)** — unless every applying claim declares `evidence: process`, in which case nothing machine-evidenced
needs a report: a policy that cannot be exercised is not a pass, so the report is part of the gate
invocation, not an optional extra.

**The rename lesson.** A test id is the test's *identity*: rename the test and the claim points at an
id the report no longer carries. The answer is *linked test not in the report* — UNVERIFIABLE, exit 3 —
never a pass, because folding a rename into PASS is how a green build stops meaning anything. When you
rename or delete a test, edit its claim in the same commit; the gate is what tells you that you did
not.

## Satisfaction links

A claim linked to a test says *the code behaves*. A **satisfaction link** says *this design element is
what meets the claim*, and keeps saying it only while the claim's meaning has not moved against it
(ADR 0040). It is optional: with no `.req/design`, nothing here runs.

1. **List the design roots** in `.req/design` — one repo-relative directory or file per line, the
   grammar of the other `.req/` lists (`#` comments, blank lines skipped; an absolute path or `..` is
   exit 2 naming the line):

   ```text
   docs/design
   ```

2. **Declare the link where the design is**, in any file under a root, one claim per occurrence. req
   reads the token, not the format, so a comment, front matter or a heading line all do:

   ```text
   <!-- req-satisfies: REQ-0007 -->
   ```

   The element's id is its repo-relative path (`docs/design/pump.md`).
3. **Record it** — the human act, never req's own: `req satisfy --record docs/design/pump.md REQ-0007`.
   That writes the claim's statement, verbatim, to `.req/satisfied`. Commit the file; it is reviewed
   like `.req/variants`.
4. **Read the answer**: `req satisfy` lists every link. A cosmetic edit to the claim leaves it
   **HOLDS**; a weakening of a class the measurement admitted (today the modality, SHALL to SHOULD,
   and a loosened bound, 100 ms to 200 ms) is **HOLDS (weakened)**, named, so the loosening is seen but not re-opened; anything else — a
   strengthening, a tighter number, a flipped polarity, any scope change, a loosening class not yet
   measured — is **SUSPECT** (exit 3, naming element, claim and direction) until a human has looked at
   the design and re-recorded it. A declared link never recorded is **UNRATIFIED**, a link to a claim
   the corpus does not hold is **UNKNOWN CLAIM**, and a record whose declaration is gone is
   **ORPHANED** (listed, not a finding).

`req gate` and `req check` carry the same answer as the `design_satisfied` policy: PASS when every link
of an applying claim holds (the line names the counts), UNVERIFIABLE otherwise — never FAIL, because a
suspect link may still comply and nobody has shown it. To require a design element for every applying
claim, add the line `need design` to `.req/completeness`; a claim no element declares is then the
structural omission `unsatisfied` (UNVERIFIABLE), also listed in `req audit`.

## Contradictions and Conflicts

Two claims about the same thing can say opposite things: "the API shall respond within 100 ms" in one file and "the API
shall respond in at least 200 ms" in another. `req gate` judges this for the claims a release applies, as its
`contradictions` policy (ADR 0043). It looks only at what Tier 1 can decide exactly — the same subject, action,
qualifier, unit, exception and temporal, under nested conditions, with a `SHALL` against a `SHALL NOT`, bounds no value
meets, or membership sets that cannot overlap — and says so: the PASS line reads *no structural contradiction among k
comparable pairs; m pairs not comparable*, never "consistent". A claim the parser leaves unstructured, a `SHOULD` or a `MAY`,
and a pair that differs in a slot are counted in *m*, not guessed. Claims that apply in different `.req/variants` lines are
never compared.

An unrecorded contradiction is a defect nobody has looked at, and the policy fails it, naming both claims, the slot and the
two values. When you (or the person who owns the requirement) have looked and know the two cannot both hold — whether or
not Tier 1 can see it — record it on either claim:

```yaml
relations:
  - type: conflicts-with
    to: REQ-0042
```

A recorded **Conflict** is information, not an error, so it is UNVERIFIABLE rather than FAIL: a release cannot be shown to
satisfy both sides of it. It is resolved by an edit, by retiring one claim, or by an applicability predicate that keeps the
two out of the same release — never by a flag, and never by deleting the relation while the two still apply. A
`conflicts-with` that names a claim the corpus does not hold is UNVERIFIABLE too. `req audit` lists the contradictions and
the applying Conflicts in a *Contradictions and Conflicts* section. A model may *propose* a Conflict (`req propose`); you
record it, or you do not.

**Replacing a requirement is a relation, not a deletion.** When REQ-0050 replaces REQ-0012 (say "at least 3" becomes "at
most 2"), add `relations: [{type: supersedes, to: REQ-0012}]` to **REQ-0050**, the new claim, and leave REQ-0012 where it is.
In every selection where REQ-0050 applies, REQ-0012 is withdrawn: the gate, `req check`, the Baseline, the audit packet and the
views no longer count it, so the pair is no longer a contradiction, and `req render` and `req select` read *does not apply
(superseded by REQ-0050)*. A successor scoped to one variant supersedes only there, and one the context cannot decide leaves
REQ-0012 undecided, which the gate names. The old claim stays readable as history, and the audit packet lists it in a *Superseded*
section. A claim that supersedes itself, or a loop of them, fails `relations_resolve`.

## CI

Save this as `.github/workflows/req.yml`. The summary step below targets GitHub's
`GITHUB_STEP_SUMMARY`, and works without it.

```yaml
name: req
on:
  pull_request:
  push:
    branches: [main]
jobs:
  requirements:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      # The install (ADR 0025): a prebuilt, checksummed binary, pinned in THIS file. Edit the three
      # pin lines at the top of the script below -- the rev and the SHA-256 of each target's tarball;
      # take them from the `.sha256` assets of the release the first time, and this file is the
      # authority from then on. The release is public, so the install needs no credential. The `env:`
      # line below matters only to a private fork or mirror, which sets the `REQ_RELEASE_TOKEN` secret to
      # a token that can read its release (an unset secret is an empty value, and then no
      # Authorization header is sent). A refusal (wrong hash, unknown rev, wrong binary) fails this
      # step and nothing falls back to a source build; req's own claim operations need no network.
      - name: req, installed from the prebuilt binary pinned below
        env:
          REQ_RELEASE_TOKEN: ${{ secrets.REQ_RELEASE_TOKEN }}
        run: |
          REV='<placeholder: the rev of the release you pin, e.g. abc1234 -- fill in after the first release>'
          SHA256_x86_64_unknown_linux_gnu='<placeholder: 64 hex digits from the release asset req-REV-x86_64-unknown-linux-gnu.tar.gz.sha256>'
          SHA256_aarch64_apple_darwin='<placeholder: 64 hex digits from the release asset req-REV-aarch64-apple-darwin.tar.gz.sha256>'
          set -eu

          # Every refusal goes through here: one line naming the cause, and the rule that there is no fallback.
          fail() {
            printf 'install-req: %s\n' "$*" >&2
            printf 'install-req: refused. There is no automatic fallback to a source install (docs/adopting.md).\n' >&2
            exit 1
          }

          # The target comes first: a platform that cannot be installed cannot be fixed by a pin.
          platform=$(uname -sm)
          case $platform in
            'Linux x86_64')
              target=x86_64-unknown-linux-gnu
              pin=${SHA256_x86_64_unknown_linux_gnu:-}
              ;;
            'Darwin arm64')
              target=aarch64-apple-darwin
              pin=${SHA256_aarch64_apple_darwin:-}
              ;;
            *)
              fail "unsupported platform: $platform (supported: Linux x86_64, Darwin arm64)"
              ;;
          esac

          # A placeholder, a typo or an empty pin is a refusal before any request is made.
          case $REV in
            '' | *[!0-9a-f]*) fail "the pin table has no usable REV (a bare lower-case hex SHA of 7 to 40 digits); fill it in from the release" ;;
          esac
          if [ "${#REV}" -lt 7 ] || [ "${#REV}" -gt 40 ]; then
            fail "the pin table has no usable REV (a bare lower-case hex SHA of 7 to 40 digits); fill it in from the release"
          fi
          if [ -z "$pin" ]; then
            fail "no pin for target $target: fill in its SHA-256 in the pin table, or install on a supported machine"
          fi
          case $pin in
            *[!0-9a-f]*) fail "the pin table's SHA-256 for $target is not 64 lower-case hex digits; fill it in from the release's .sha256 asset" ;;
          esac
          if [ "${#pin}" -ne 64 ]; then
            fail "the pin table's SHA-256 for $target is not 64 lower-case hex digits; fill it in from the release's .sha256 asset"
          fi

          for tool in curl tar; do
            if ! command -v "$tool" >/dev/null 2>&1; then
              fail "$tool is required and was not found on PATH"
            fi
          done
          if command -v sha256sum >/dev/null 2>&1; then
            sha_tool=sha256sum
          elif command -v shasum >/dev/null 2>&1; then
            sha_tool=shasum
          else
            fail "a SHA-256 tool (sha256sum or shasum) is required and was not found on PATH"
          fi

          dir=${REQ_INSTALL_DIR:-$HOME/.local/bin}
          mkdir -p "$dir"
          # Staged inside the install directory: the same filesystem (the final move is a rename) and a
          # place that is known to allow execution. Removed on every exit, so a refusal leaves nothing.
          stage=$(mktemp -d "$dir/.req-install.XXXXXX")
          trap 'rm -rf "$stage"' EXIT

          base=${install_base:-https://github.com/autox-my/req/releases/download}
          proto=${install_proto:-=https}
          asset=req-$REV-$target.tar.gz
          url=$base/rev-$REV/$asset

          # The header exists only when a token was given (a public release needs none). It is built by the
          # shell builtin `printf` and read by curl from stdin: the token is never in an argument list (`ps`),
          # and nothing here is traced. With no token the header file is empty and no Authorization is sent.
          status=0
          code=$({ if [ -n "${REQ_RELEASE_TOKEN:-}" ]; then printf 'Authorization: token %s\n' "$REQ_RELEASE_TOKEN"; fi; } |
            curl --proto "$proto" --tlsv1.2 --retry 3 -fsSL -H @- -o "$stage/$asset" -w '%{http_code}' "$url") || status=$?
          case $code in
            2??)
              if [ "$status" -ne 0 ]; then
                fail "download failed (curl exit $status) for $url"
              fi
              ;;
            401 | 403)
              fail "HTTP $code for $url: the host refused the request; if the release is private, check that REQ_RELEASE_TOKEN is valid and can read it, otherwise unset REQ_RELEASE_TOKEN"
              ;;
            404)
              fail "HTTP 404 for $url: there is no release rev-$REV for $target, or (for a private fork) the token cannot see it; check REV and REQ_RELEASE_TOKEN"
              ;;
            *)
              fail "HTTP $code (curl exit $status) for $url"
              ;;
          esac

          # Verified against the PIN above, never against a checksum fetched from the release.
          sha_ok() {
            if [ "$sha_tool" = sha256sum ]; then
              printf '%s  %s\n' "$pin" "$asset" | (cd "$stage" && sha256sum -c -) >/dev/null 2>&1
            else
              printf '%s  %s\n' "$pin" "$asset" | (cd "$stage" && shasum -a 256 -c -) >/dev/null 2>&1
            fi
          }
          if ! sha_ok; then
            fail "SHA-256 mismatch: $asset does not match the SHA-256 pinned for $target"
          fi

          # One member, `req`, a regular file, at the archive root: no prefix, no second member, no `..`,
          # no absolute path, no link. Checked from the listing, before anything is extracted.
          members=$(tar -tzf "$stage/$asset" 2>/dev/null) || members='(unreadable archive)'
          kind=$(tar -tvzf "$stage/$asset" 2>/dev/null | cut -c1) || kind='?'
          if [ "$members" != req ] || [ "$kind" != - ]; then
            fail "$asset must hold exactly one member named req, a regular file at the archive root"
          fi
          mkdir "$stage/unpacked"
          tar -xzf "$stage/$asset" -C "$stage/unpacked"
          chmod 0755 "$stage/unpacked/req"

          # The binary is run from the staging directory and installed only if it is the pinned rev.
          version=0
          out=$("$stage/unpacked/req" --version 2>&1) || version=$?
          if [ "$version" -ne 0 ]; then
            fail "the downloaded req does not run (exit $version): $(printf '%s' "$out" | head -c 300 | LC_ALL=C tr -c '[:print:]' '?')"
          fi
          built=$(printf '%s\n' "$out" | sed -n 's/^ *built from  *\([^ ]*\) *$/\1/p')
          if [ "$built" != "$REV" ]; then
            fail "the downloaded req reports built from '${built:-(no such line)}', not the pinned $REV"
          fi

          mv -f "$stage/unpacked/req" "$dir/req"
          printf 'installed %s\n' "$dir/req"
          printf '%s\n' "$out" | sed -n -e '/^ *rules_v /p' -e '/^ *built from /p'
      - name: put the installed req on PATH for the steps that follow
        run: echo "${REQ_INSTALL_DIR:-$HOME/.local/bin}" >> "$GITHUB_PATH"
      # The tests that prove the claims (ADR 0019). This template is Rust and uses nextest because
      # it writes the JUnit/XUnit document `req gate --test-report` reads; on another stack, run
      # your own test command and point `--test-report` at the report it writes. A claim with no
      # linked test is a finding, never a pass — the gate below names it.
      # No stale report may answer for this run. `if: always()` is the point: when an earlier step
      # fails, every ordinary step after it is skipped — and on a cached or self-hosted workspace
      # yesterday's `target/nextest/ci/junit.xml` would then survive for the artifacts step below,
      # which would publish it as a `pass` beside a red build.
      - name: invalidate any stale test report
        if: always()
        run: rm -f "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml"

      - name: the tests that prove the claims, as the report the gate reads
        run: |
          set -eu
          cargo install cargo-nextest --version 0.9.149 --locked
          # The reviewed profile, only when the project does not already own one: fail-fast off,
          # so one failure cannot hide the rest, and the JUnit report with skipped tests present
          # (a skipped test must be visible — it is not a pass).
          if [ ! -f .config/nextest.toml ]; then
            mkdir -p .config
            cat > .config/nextest.toml <<'TOML'
          [profile.ci]
          fail-fast = false

          [profile.ci.junit]
          path = "junit.xml"
          report-skipped = "ignored"
          TOML
          fi
          # The profile and the report path are what `--profile ci` and `--test-report` assume. When
          # the project already owns a config, verify it declares both — do not assume: a config
          # without `[profile.ci]` makes `--profile ci` fail, and one without `[profile.ci.junit]`
          # writes no report at all, so the gate would read a missing file and answer UNVERIFIABLE
          # beside tests that passed. The likely adopter fix is to drop `--test-report`, which relaxes
          # the policy — so this is a named error, not a puzzle.
          if ! grep -q '^\[profile\.ci\]' .config/nextest.toml \
             || ! grep -q '^\[profile\.ci\.junit\]' .config/nextest.toml \
             || ! grep -q 'path = "junit.xml"' .config/nextest.toml \
             || ! grep -q 'fail-fast = false' .config/nextest.toml; then
            echo "::error::.config/nextest.toml exists but does not declare [profile.ci], [profile.ci.junit] and path = \"junit.xml\": add them, or point --test-report at the report your own test command writes"
            exit 1
          fi
          cargo nextest run --profile ci
      # The release verdict (ADR 0023). `req check` reads `.req/variants` and `.req/projections`
      # beside the corpus, runs one `req gate` per release line and one `req render --check` per
      # pinned projection, and folds them into ONE exit: 1 when any step is a defect, else 3 when
      # any cannot be confirmed, else 0; 2 for a malformed list or a usage error. The fold, the
      # list grammar and "an absent or unreadable list is never silent" are req's, tested once, so
      # this step re-implements none of them and adds only the job summary.
      #
      # The step's status IS `req check`'s: the command is not piped (a pipe reports the last
      # command's status), not followed by `|| true`, and the step is not `continue-on-error`. The
      # output goes to a file so the summary can quote its `verdict:` line, and the step ends by
      # exiting with `code`, taken from the command's own status — an exit 3 (UNVERIFIABLE) is a
      # red step, never a green one.
      - name: the release check — every declared variant and projection, one verdict
        run: |
          set -u
          out="${RUNNER_TEMP:-$HOME}/req-check.out"
          if req check --test-report "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml" > "$out" 2>&1; then
            code=0
          else
            code=$?
          fi
          cat "$out"
          # A job summary above the log. GitHub sets GITHUB_STEP_SUMMARY; a run without it (this step
          # run by hand) must still pass, so the target falls back to /dev/null.
          summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
          verdict="$(sed -n 's/^verdict: //p' "$out" | head -1)"
          {
            printf '## req check\n\n| exit | verdict |\n| --- | --- |\n| %s | %s |\n\n' "$code" "${verdict:-no verdict line}"
            printf '```\n'
            cat "$out"
            printf '```\n'
          } >> "$summary"
          exit "$code"
      # The agent-facing half (pack C): the verdicts as JSON documents, kept off the log.
      # An agent reviewing this pull request reads `req.check/1` (one document: the folded verdict
      # and, per step, the whole `req.gate/2`), never a scraped text line.
      - name: the JSON views, as build artifacts (an agent reads these, not the log)
        if: always()
        run: |
          set -eu
          mkdir -p req-views
          # A view document is complete even when its verdict is open (pack C), so each exit
          # class is recorded beside its document and never folded into this step: the check
          # above decides the build, and `always()` keeps the views when it went red.
          # `--format json` renders to stdout only — beside `--out`/`--check` it is a usage
          # error (exit 2) — so a redirect, not a flag, is how a document reaches the disk.
          #
          # The redirect goes to a `.tmp` name first because a command can fail WITHOUT writing
          # a document: an unreadable corpus prints `[UNVERIFIABLE]` to stderr and exits 3, and
          # a bare `> req-views/check.json` would leave a zero-byte file where a view should be.
          # A consumer that parses an empty document believes it holds a verdict. So a view
          # that wrote nothing is named on the log and leaves no file at all.
          capture() {
            name=$1
            shift
            req "$@" --format json > "req-views/$name.json.tmp" && code=0 || code=$?
            if [ -s "req-views/$name.json.tmp" ]; then
              mv "req-views/$name.json.tmp" "req-views/$name.json"
              printf 'req %s --format json -> req-views/%s.json (exit %s)\n' "$*" "$name" "$code"
            else
              rm -f "req-views/$name.json.tmp"
              printf 'req %s --format json -> NO DOCUMENT, nothing written (exit %s)\n' "$*" "$code"
            fi
          }
          capture render render
          capture trace trace
          capture audit audit
          capture check check --test-report "${CARGO_TARGET_DIR:-target}/nextest/ci/junit.xml"
          # `if-no-files-found: error` on the upload only reds a COMPLETELY empty directory: one
          # document lost while the check passed would ship silently. The step knows how many
          # documents it expects (render, trace, audit and check), so it checks instead of assuming.
          expected=4
          actual=0
          for document in req-views/*.json; do
            [ -f "$document" ] && actual=$((actual + 1))
          done
          if [ "$actual" -ne "$expected" ]; then
            printf 'incomplete view set: expected %s document(s), found %s\n' "$expected" "$actual" >&2
            exit 1
          fi
      - name: keep the views for the reviewing agent
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: req-views
          path: req-views/
          if-no-files-found: error
```

**The install is the prebuilt binary, pinned in this file.** The first step is
`examples/install-req.sh`'s body verbatim (a test keeps the three copies and the script identical),
pin table first: edit the rev and the SHA-256 of each target's tarball, which ship as placeholders
that fail closed. The release is public, so the install needs no credential and replaces the SSH deploy
key and the Rust toolchain for *req*; the `env:` line that maps the `REQ_RELEASE_TOKEN` secret matters
only to a private fork or mirror (*Install req*), whose token is sent as a header read from stdin,
never on a command line and never printed, and only when it is set. Every refusal — a wrong hash, an unknown rev,
a binary that is not the pinned rev — fails the step: it is not `continue-on-error`, not followed by
`|| true`, and does not fall back to a source build. The second step adds the install directory (`${REQ_INSTALL_DIR:-$HOME/.local/bin}`, the one the install
used) to the runner's `PATH` through `$GITHUB_PATH`, which applies to *later* steps only (the install runs its own
binary by path). No deploy key and no source build is involved; the source install stays documented
under *Install req* for a project that wants it. req's own claim operations need no network (REQ-0012).

**The release step is one command: `req check`.** It reads the two lists beside the corpus —
`.req/variants` (one complete release per line) and `.req/projections` (one committed document path
per line) — runs a `req gate --test-report <junit.xml> <line>` for each release and a
`req render --check <path>` for each projection, and folds the results into **one exit with the
gate's contract**: **1** when any step is a defect, else **3** when any step cannot be confirmed,
else **0**; **2** for a usage error or a malformed list, decided before any step runs. The step's
status *is* that exit: the command is **not piped, not followed by `|| true`, and the step is not
`continue-on-error`**, because a pipe reports the last command's status and a masked exit 3 is a
green build over an unconfirmed release. The step keeps a copy of the output only to quote its
`verdict:` line in the job summary, and ends in `exit "$code"` with `code` taken from the command's
own status. Its output says which step produced which exit, and `verdict: PASS` is printed only when
**every declared step ran and passed** — there is no `--only` and no `--skip`, so a green `check` is
never a green subset. If the project's tests write more than one report (nextest, pytest, the
`tools/junit_case.py` adapter), repeat `--test-report` once per report on the `req check` line: the
template below passes one, which stays valid for a single report.

**Absent is not silent.** A repository with no `.req/variants` declares the default variant — one
bare `req gate` — and `req check` says so on its own line; on a corpus carrying a selector that gate
is **3** (ADR 0015), so deleting the list cannot turn the build green. A list that exists and cannot
be read as a regular file (a directory, a dangling link, a mode-000 file) is **3**, never "absent".
A comment-only `.req/variants` is the default variant, and says so. With no `.req/projections` the
command prints `projections: none pinned` and checks none; a listed path that is **missing** (or not
a regular file) is a failure — a projection that vanished is silent decay (ADR 0018) — and a list
that yields no path is a failure too, because it examined nothing.

**The grammar of both lists is `req check`'s**, tested once in Rust rather than reviewed in three
copies of shell: `#` starts a comment, an empty line is skipped, a trailing carriage return is
stripped, a line is split on whitespace with **no pathname expansion** (a `*` in a value is data),
and a final line without a newline counts. A `.req/variants` line is `key=value` words and **never an
option or a word without `=`** — `req gate --help` exits 0 having gated nothing, so an option-shaped
line is rejected (exit **2**, naming `<file>:<line>`, before anything runs) rather than becoming a
help page that "passes". A `.req/projections` line is exactly **one path**, relative to the corpus
root; an absolute path or a `..` component is malformed, because the list is repository content and
must not point outside it. What a line *means* is unchanged.

**Structural omissions (ADR 0039).** `req check` always runs one more step, `completeness`, over the
release lines, the claims and `baselines/`:

- **`in_no_release` (1).** A claim every `.req/variants` line excludes: nothing examines it. Retire it,
  or declare the release that ships it.
- **`in_no_baseline` (3).** A claim a release ships that no **signed** manifest under `baselines/` names.
  A manifest counts only when its `.minisig` lies beside it. Whether the signature verifies is
  `req baseline --verify`'s answer, so verify every committed manifest in the same CI (this repository's
  `scripts/check.sh` does). **Until you sign a first Baseline, `req check` is 3**: nothing is ratified,
  which is what `req audit` already says. Sign one per release line (*Sign the Baseline*).
- **`unrelated` (listed only).** A claim with no relation in or out. Add the missing `refines` or
  `derives-from`, or declare it a root in `.req/completeness`.

`.req/completeness` is optional and has the same grammar as the other two lists. Each line is one root
claim id, `strict unrelated` (which turns every unlisted unrelated claim into a 1) or `need design`
(which adds `unsatisfied`, a 3 for an applying claim no design element declares: *Satisfaction links*). An unknown word or
an id the corpus does not hold is exit 2. A claim a line leaves undecided is that gate's 3 and is not named
here again. None of this certifies the requirements **complete**: req holds no hazard list, stakeholder
needs or standard clauses to be complete against.

**The JSON views are the agent-facing half.** The last two steps write pack C's documents and
upload them as the `req-views` artifact, so an agent reviewing the pull request reads the verdict
instead of scraping a log. `render`, `trace` and `audit` are written once, and **`check.json` is one
`req.check/1` document**: the folded verdict and, per step, its kind, its line or path, its exit and,
for a gate step, the whole `req.gate/2` document — one document where a per-variant capture used to
need a loop. The capture step runs `if: always()` and **records** each command's exit class beside
its document instead of folding it into the step: a document is complete even when its verdict is
open (ADR 0018, and pack C's rule), and the `req check` step above is what decides the build.
`--format json` renders to stdout only — beside `--out`/`--check` it is a usage error (exit 2) —
so the redirect is how a document reaches the disk, and it goes to a `.tmp` name first: a view that
fails **without writing a document** (an unreadable corpus prints `[UNVERIFIABLE]` and exits 3, a
malformed list exits 2) leaves no file and is named on the log, because a zero-byte document is the
one thing a consumer would parse and believe. The step knows it expects **four** documents
(`render`, `trace`, `audit`, `check`) and is red on a partial set. `tests/ci_template.rs` pins
`if: always()`, `if-no-files-found: error`, the captures and the set check by **count**, so a
coordinated removal from all three copies is still red.

The upload names `actions/upload-artifact@v4`. That version is the one **runner-specific** line in
this file: check the major your runner implements (GitHub Enterprise Server may lag the hosted runners) and change it if it differs —
the JSON files are written to `req-views/` either way, and only the upload is version-specific.

**This body is one text in three places**, and a test keeps it that way: `req init` prints it,
this page carries it, and `examples/github-workflows/req.yml` is the copy-ready file to `cp` into
`.github/workflows/`. `tests/ci_template.rs` compares the three byte for
byte — the equality is checked, never assumed — and refuses a copy that carries the old shell
(a loop over `.req/variants`, `read -a`, a sticky `rc`) back.

**`.req/variants` and `.req/projections` are plain text beside the corpus**, the repository's to
author and `req check`'s to read. Two lines you will write often:

- **Declare the default variant by declaring nothing.** A comment-only `.req/variants` describes
  exactly one variant — the default — and `req check` runs the bare-variant `req gate` (still with
  the report) for it. Once a line is declared, **the lines are the gate**: a bare run beside them
  would be UNVERIFIABLE on any corpus carrying a selector (ADR 0015), and a false red beside green
  variants is how a real red gets ignored.
- **Add a variant in the same commit as the `applicability:` that needs it** (*The coverage rule*).

Two parts of the snippet are not verified on a runner: whether the job summary renders above the log, and
the artifact upload — `actions/upload-artifact` and `if: always()` are the runner's behaviour, not req's.
Everything else is checked rather than described: `tests/ci_template.rs` **runs the release step's own
command line** against corpora whose verdict is known (a fixture that must be exit 3 must make the step
exit 3), `tests/init.rs` executes the step and the JSON-views step against a stub `req` (every exit code
passes through; a view that wrote nothing leaves no file and the step goes **red** because it knows how
many documents it expects), and `tests/check.rs` holds the fold and the list grammar. Confirm the table
renders on your first pull request; if it does not, the step is still correct, it just loses the summary.

## On a pull request

`req check` (the CI section) answers *may this repository ship*. It does not answer the question a
reviewer has on a pull request: **across this change, which obligations moved, in which direction, and did
anything get quietly weaker?** That is `req diff`, and it is a description of a change, not a verdict on it
(ADR 0026). It never prints PASS or FAIL; whether a movement is acceptable is the owner's decision.

```text
req diff --base origin/main --fail-on weakened,removed,pins-changed,tests-lost
```

`--base` is a git revision; `--head` is a git revision, or the working tree when absent (an uncommitted
edit is read). Claims are matched by `claim_id`, and each id on either side gets one **class**. A claim
that is identical on both sides is counted, not listed.

| class | meaning |
| --- | --- |
| `added`, `removed` | the id exists on one side only |
| `weakened` | the meaning moved so that strictly more complies: modality weakening, numeric loosening, membership widening, a condition added to a SHALL, SHOULD or SHALL NOT (it binds less often: `scope_narrowing`) |
| `tightened` | the reverse: modality strengthening, numeric tightening, membership narrowing, a condition removed (`scope_widening`) |
| `reshaped` | a different obligation, not a stronger or weaker one (polarity flip, scope divergence, unrelated subject or predicate, a condition changed on a MAY, a change on several axes that point different ways): read it |
| `not-judged` | the meaning changed but one side has no structured form, so no direction is claimed |
| `reworded` | the statement text changed and the meaning fingerprint did not |
| `metadata` | only fields outside identity changed: relations, rationale, tests, evidence, pins |

Beside its class a claim carries **flags**:

| flag | meaning |
| --- | --- |
| `pins-changed` | the authored `pins:` differ, compared structurally. An edited pin is how a flagged parse gets silenced, so it is worth a look in its own right |
| `pin-flagged` | a pin on the **head** side disagrees with the parse. A pin that already disagreed at base and is otherwise unchanged is not listed |
| `tests-lost`, `tests-added` | a linked test id present on one side only |
| `same-address-as <id>` | an `added` claim has the meaning fingerprint of a `removed` one: a candidate renumber, reported as a candidate and never inferred as a rename (both stay `added` and `removed`) |

A `contradictions:` section follows the claims: the structural contradictions (ADR 0043) the change **added or resolved**, each with its `.req/variants` line, both ids and the two values. Contradiction belongs to a selection, so it is judged once per line of the **head's** list on both sides (no list is the bare selection), and each claim in an added pair carries the flag `contradiction-added <other-id>`, which `--fail-on contradiction-added` turns into exit 1; an unchanged claim is then listed so the flag shows.

A line is `<class> <claim_id> [<direction>] [flags…]`, sorted by claim id; the direction word is the one
`req compare` prints. From a real run, where a claim went from SHALL to SHOULD and its bound from 200 ms to
300 ms:

```text
# req diff: HEAD~1 (28c74120701e) -> HEAD (6f0e62468662)
registry: unchanged
weakened REQ-0001 modality_weakening pin-flagged
contradictions: none added or resolved across 1 selection(s)
counts: added 0, removed 0, weakened 1, tightened 0, reshaped 0, not-judged 0, reworded 0, metadata 0, unchanged 0
fail-on: weakened,removed,pins-changed,tests-lost — 1 triggered
```

**`--fail-on` decides what blocks a merge.** Its value is a comma-separated subset of `weakened`, `removed`,
`reshaped`, `not-judged`, `pins-changed`, `pin-flagged`, `tests-lost`, `contradiction-added`; any other spelling, or an empty list, is
exit 2. Nothing is on by default, and an unlisted class is still printed. A reasonable first set is
`weakened,removed,pins-changed,tests-lost`: the movements a team most often wants a second pair of eyes on.
Widen it (`reshaped`, `not-judged`, `pin-flagged`) once the reports have been read for a while, or narrow it;
a class that does not occur is never a failure. `--format json` prints one `req.diff/1` document with the same
classes.

The exit is **0** when the report was produced, whatever it lists; **1** only with `--fail-on`, when a named
class or flag is present; **3** when it cannot be produced (UNVERIFIABLE); **2** for a usage error. Read the
exit before the output: a report that could not be produced is never an empty one.

What to keep in mind when reading it:

- **`weakened` is a prompt for a human, not a rejection.** Loosening a requirement is sometimes the right
  decision. The point is that it is made on purpose, by someone who read it, and not discovered later in a
  YAML hunk. req never merges, ratifies or approves.
- **The direction is exactly as good as `req compare`'s.** The classifier is Tier-1 structure (modality,
  comparator and bound, scope). A pair it cannot read is `not-judged`, not a quiet "changed"; a pair it
  reads wrongly is a `req compare` defect, and the place to report it.
- **A registry change is reported once, at the top** (`registry: changed`). An edit to `aliases/subjects.yml`
  can move many addresses at once; read the claims below it in that light, because they are not many edited
  statements. Each side is derived under **that side's own registry** and this binary's rules.
- **It needs `git`, and the base revision in the checkout.** req runs the local `git` binary as a process
  boundary, with its `GIT_*` redirections and its user and system configuration removed, and reads only
  `claims/` and `aliases/` as blobs, without checking anything out. Nothing is fetched and no hook runs. On a
  runner this means `fetch-depth: 0` on the checkout: with a shallow clone `origin/<base>` is not there, and
  `req diff` is exit 3 (the revision does not resolve). The corpus must be inside a git work tree.

`examples/github-workflows/req-diff.yml` is the copy-ready workflow: on `pull_request`, a checkout with
`fetch-depth: 0`, the same prebuilt install as the CI section's (the pin table is the one thing you edit),
then `req diff --base "origin/$BASE_REF"` (the branch name arrives through `env:`, never interpolated into the script) with the starting set above. It is a separate file
from `req.yml` and runs beside it. The command is unpiped and unmasked, so its exit is the step's; its
output is also written to the job summary from a captured copy. Posting a comment, labelling the pull
request or reading a forge's API is a CI step's job, not req's.

A pull request from a fork has no access to the repository secrets, so the install step refuses and the check is red on
it; that is deliberate (the trigger is `pull_request`, never `pull_request_target`).

## A corpus kept in another repository

When the claims live in one repository (the policy) and the code in another, `req --root ../policy gate …` gates the code against the
policy. Pin the policy's commit in the code repository, so a green run says *which* policy it passed (ADR 0033).

The pin file is two lines, committed in the code repository (`corpus.pin` here; any path works):

```
# the policy this repository is gated against
repo: https://github.com/example/policy
rev: 0123456789abcdef0123456789abcdef01234567
```

`rev` is the policy's **full** 40-digit commit SHA: a short SHA is ambiguous and a branch or tag moves. `req` never fetches, so your CI
puts that commit in the checkout and then asks req whether the checkout *is* the pin, before any gate runs:

```bash
set -euo pipefail
git clone --quiet https://github.com/example/policy.git ../policy      # a private policy needs the CI's own token
git -C ../policy checkout --quiet "$(sed -n 's/^rev: *//p' corpus.pin)"
req --root ../policy pin --verify corpus.pin     # prints "corpus pinned: <repo> @ <sha>"; 1 if not that commit or dirty; 3 if it cannot be told
req --root ../policy gate --test-report target/nextest/ci/junit.xml deployment_mode=air_gapped
```

`pin --verify` is exit 0 only when `HEAD` is the pinned commit and `claims/` and `aliases/` have no uncommitted or untracked change;
the line it prints is the first thing the job's log shows. A pinned commit that is not in the checkout, a pin file that is missing or
malformed, or a corpus outside a git work tree is exit 3, never a pass.

To see how far the policy has moved on, in a scheduled job or a pull request that wants to move the pin:

```bash
git -C ../policy fetch --quiet origin main      # a shallow clone needs fetch-depth: 0
req --root ../policy pin --drift corpus.pin --against origin/main
```

This is `req diff --base <pin> --head origin/main` in the policy repository: each changed claim with its class (added, removed,
weakened, tightened, reshaped, …), identical claims counted, no verdict. Moving the pin is then a pull request in the code repository
that changes one line and carries that output. The pin is not signed; the policy's own Baseline is verified as the policy's, against the
pinned checkout.

## The coverage rule

`req gate` checks the **applying claims of its invocation**. A claim the invocation leaves
**undecided** is not examined by it — and since ADR 0015 that is a failure, not a disclosure: the
gate exits **3 (UNVERIFIABLE)** and names the claims it could not examine, with the remedy. A bare
`req gate` is therefore UNVERIFIABLE on any corpus carrying a selector, and the per-variant
invocation is the only one that can be green. So:

**Every variant the project declares gets its own gate invocation.** That is what `.req/variants`
is for, and `req check` runs one `req gate` per line — and since ADR 0019 each invocation also takes
the run's report (`--test-report target/nextest/ci/junit.xml`), because without it the `tests_linked`
policy cannot be exercised and the gate is UNVERIFIABLE rather than green. When you add an
`applicability:` predicate to a claim, add its variant to `.req/variants` in the same commit —
otherwise the claim is inside no gate invocation at all, and the bare run tells you so: it exits 3
and names that claim among the undecided.

## Without CI

Nothing about the tool changes; only whether it is run. The gate becomes a ritual, and a ritual that
is not enforced is not a gate. The cheapest enforcement is a `pre-push` hook that runs the same one
command as CI — `req check --test-report target/nextest/ci/junit.xml` — and fails on its status; on a
corpus carrying a selector a bare `req gate` is UNVERIFIABLE (ADR 0015), so the declared lines *are*
the gate, and `req check` runs every one of them and every pinned projection. Run the suite first
(nextest, one profile, one report) or the gate is UNVERIFIABLE for lack of evidence. Do not
oversell it: a local hook is a reminder, not a record, and nothing outside your machine can see
whether it ran.

## For an agent

The same path for an agent and the human with it; the unit is the **claim** — one obligation, one file.
`tools/quickstart.sh` rehearses it offline in a scratch directory: it prints `QUICKSTART <n>s` (1s on the
last measured run, 0-2s across runs) and fails at n >= 300, so "under five minutes" is an instrument, not
prose. The script does not install req — the install is the adopter's step and is not in the measured
window. Each step names its deciding line; the explanation is in the section it points at.

1. **Install at a pinned rev** (*Install req*). Exit **0**, `req --version` printing `rules_v` and
   `built from` equal to the rev you pinned; for the source install `<PIN>` is read with
   `git rev-parse --short origin/main`, never copied out of a document.
2. **Start the corpus** (*Start a corpus*): `req init`, exit **0**, first line `Wrote a working, verified
   corpus in the current directory:`, exactly three files. Confirm with `req canon` → exit **0**, last line
   beginning `claims: 1 (0 unstructured); pins: 3 on 1 claim(s), 0 flagged`. Its refusals are on
   `req init --help`.
3. **Bring your own spec** (*Adopting from documents you already have*): copy `tools/adopt.py` out of the
   req checkout at the rev you installed and run it from the corpus root. A **dry run** is the default and
   prints, exit **0**:

   ```text
   draft  REQ-0002  claims/REQ-0002.yml  docs/capabilities.md#L<n>
   skip   docs/capabilities.md#L<m>  empty text
   dry run: nothing written — pass --write to write them
   ```

   `--write` writes the drafts and names them (`wrote: <n> claim(s) — claims/REQ-0002.yml, …`).
4. **Ratify: read the parse, pin the bound** (*The authoring loop and its traps*): `req canon` reports and
   by default never gates — one row per claim with its derived parse, one `pin <key> <value>: agrees` (or `FLAG …`)
   line per pin, and a last line `claims: <n> (<u> unstructured); pins: <p> on <c> claim(s), <f> flagged; …`.
   Nothing is filled: there is nothing derived to write. A flagged pin is the gate's to fail (`pins_hold`), and
   the gate cannot be green until every applying claim has a passing linked test or declares `evidence:
   process` — so until then wire `req canon --fail-on-flag` into CI: exit 1 when any pin of any claim is
   flagged, 0 when none is, 3 when the corpus cannot be read. Its green is **not** a release verdict (tests,
   relations and decisions were not judged); replace it with `req check` at step 5, do not add it to the
   release path as a substitute.
5. **Gate it** (*Link each claim to its proving test*, *CI*, *The coverage rule*): declare each release in
   `.req/variants`, run the suite for the report, then `req check --test-report
   target/nextest/ci/junit.xml` — one command that runs a `req gate` per line, checks every path in
   `.req/projections` and folds them into one exit (**0** every declared step ran and passed, **1** a
   defect, **3** something cannot be confirmed, **2** a malformed list). With every applying claim linked
   to a passing test it prints one `[PASS] gate <line> (exit 0)` per release and ends `verdict: PASS —
   every declared step ran and passed`; a step that is not a pass lists the policies that were not PASS
   under it. An undecided claim exits **3** (since ADR 0015) and a missing `.req/variants` is the
   default variant, which on a corpus carrying a selector is also **3**: that is correct, not a bug.
   The single gate for one selection is still `req gate --test-report target/nextest/ci/junit.xml
   <key=value ...>` (first line `# Release gate: <n> applying claim(s), 0 undecided …`, last
   `verdict: PASS — every policy settled in the affirmative, and every claim was decided`). Commit the
   document as a projection (*The document your project can commit*): `req render --out docs/spec.md`,
   list it in `.req/projections`, and `req check` is the alarm.

To hand **one** of these steps to a coding agent in another project, fill in `examples/handoff-brief.md`: it
names the step, the req rev to precheck with `req --version`, the input file, the exit code and line that
mean done, and what the agent reports back. The agent finds the commands from `req --help`, not from you.

### What the agent may not do

Paste the block in `examples/agents-snippet.md` into your project's own `AGENTS.md` — it is this list, cut
for an adopter's agent. The four prohibitions, and why each one matters:

- **Never add a derived field to a claim file.** The parse, the canonical form and the address are derived at load and stored nowhere (`req.claim/2`); a file carrying `address`, `canonical` or any removed slot is a parse error that names the key. State a load-bearing slot as a pin instead — and never edit a pin to match a parse the statement was not meant to have.
- **Never edit a committed projection.** Regenerate it with `req render --out`; `req render --check` is what proves it current. An edit moves bytes the corpus did not produce, and `--check` reports it.
- **Never treat `req canon`'s exit 0 as a pass.** It reports the derived parse and each pin's verdict and does not fail on a flag; the check that gates is `req gate`.
- **Never auto-merge.** A change direction is classified for a human to ratify, two addresses that differ are different requirements, and ratification is the human's act (ADR 0004).

### What it can read

`--format json` on `render`, `trace`, `gate`, `audit` and `check` prints the same answer as a document on
stdout — `req.render/1`, `req.trace/1`, `req.gate/2`, `req.audit/1`, `req.check/1` — with the exit code
identical to the text form. It renders the verdict, it does not add one. The CI Action writes four of them
to `req-views/` (see *CI*): `check.json` is **one** document for the whole release check, carrying every
step's `req.gate/2` inside it. A `req.gate/2` document carries **eight** `policies` (the first is
`pins_hold`) and a top-level `process_evidenced` array, so a consumer that reads only the machine count
would overstate what was proven. Read the artifact, not the log; the `req check` step remains what decides
the build.

The exit contract is on `req --help` and each command's `exit:` line: an agent consumes the exit code and
the named line, not the prose. `req render --check <path>` is the drift alarm — 0 current, 1 stale or
hand-edited, 3 cannot judge; treat 3 as not current, and never as a pass.

### The honest limit

`req gate --test-report` says **which claims have no passing test** — named per claim, exit 3, never folded
into a pass. It does not say that a linked test *proves* the claim: a link is a claim about evidence,
attested in the signed Baseline, and reviewed like a rationale. A `process` declaration is evidence of a
different kind: the gate can check that it is consistent and attested, never that the process check was
run — so it is reported as declared, not as proven. A wrong link is worse than no link, because it
manufactures confidence; `req audit`'s coverage is not test coverage either. And the report is an input,
not a verdict — an absent, unparsable or empty one is UNVERIFIABLE, never "no failures found". One gap is
named rather than hidden: nextest does not run doctests, so a future `#[doc]` example needs its own gate
step.

## Reporting a bug in req

Report it at <https://github.com/autox-my/req/issues/new/choose> with the **Tool defect** form, which asks for what follows and
labels the issue `tool-defect`. The repository is public: reduce the problem to the smallest statement that shows it, never
paste confidential requirement text. A maintainer adds **`wrong-verdict`** to the two failures that matter most, a requirement
change read as no change and a pass where nothing could be checked, and **`noise`** to the survivable ones (a false split, a
wrong lint). Open `tool-defect` issues are the project's known-problems list. Include four things, because each one
changes the triage:

1. the claim `statement` you were authoring, or the corpus file if it is a parsing problem;
2. the exact command you ran;
3. its exit code and the output line that decided it;
4. `req --version` — `rules_v` decides whether a stored address is current, so a verdict is not
   reproducible without it.

A verdict has three states, always: **PASS**, **FAIL** and **UNVERIFIABLE**. A false merge (two
different requirements collapsing to one address) is the failure this tool exists to prevent, so
report it first; a false split is noise by comparison.

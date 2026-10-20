//! `req init` — create a working corpus in the **current working directory**.
//!
//! Every other subcommand *reads* a corpus from a runtime-resolved root (ADR 0012); `init`
//! is the one that is asked to create one, so it resolves nothing: it is answered in
//! `main.rs` in the same position as `--version`/`--help`, before the walk-up, and it never
//! prints a `corpus root:` line. The anchor is therefore the working directory and never an
//! ancestor — the two ways to get that wrong are both worse than a usage error, so they are
//! made impossible rather than documented:
//!
//! * it does **not** search ancestors for an existing corpus, because a project started
//!   inside another one must be able to start; and
//! * it refuses to run where `claims/` or `aliases/subjects.yml` already exists, or where
//!   `aliases` is a symlink (exit **1**), because the alternative is overwriting a human's
//!   corpus — or writing it into a tree the caller never named;
//! * `--root` is a **usage error** (exit **2**). Silently ignoring it would create a corpus
//!   somewhere the caller did not ask for, and honouring it would give `init` a second,
//!   differently-anchored meaning of a flag ADR 0012 defines as *the corpus root to read*.
//!
//! Exit codes: **0** created, **1** refused, **2** usage, **3** UNVERIFIABLE (the target
//! cannot be written, or the starter claim could not be verified). The 3 is the same
//! three-state rule as everywhere else: "we could not establish a green corpus" is neither
//! a creation nor a refusal.
//!
//! # The starter claim is filled by the real path, and asserted green
//!
//! `canonical.form` and `address` are **derived**, so `init` derives them with exactly the
//! code `req canon --fill` runs — [`evaluate_claim`] → [`canonical_json`] → [`address_hex`]
//! → [`fill_text`] — and then asserts the result with [`verify_text`], the same check
//! `req canon --verify` performs. A literal digest in this file would pass every test today
//! and go stale the moment `RULES_V` moves; the derivation cannot. If the assertion fails,
//! nothing is left behind: a half-written corpus would be red *and* refuse the next `init`.
//!
//! `rules_v`, the file schema and the registry version come from the constants that carry
//! them ([`RULES_V`], [`crate::canon::claim_schema`], [`REGISTRY_V`]), never from literals
//! typed into a template string.
//!
//! The rationale `date:` is the date `init` runs — provenance is written, not asked for —
//! computed from `SystemTime` with the civil-from-days conversion in this module (no new
//! dependency), which is unit-tested against known epoch dates.

use crate::canon::{
    RULES_V, address_hex, canonical_json, claim_schema, evaluate_claim, load_claims, load_registry,
};
use crate::fill::{StoredState, fill_text, verify_text};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The three paths `init` writes, relative to the working directory. Exactly these: a
/// scaffold that also guessed a framework would be an opinion about the project, not about
/// its requirements.
const CLAIM_PATH: &str = "claims/REQ-0001.yml";
const CLAIMS_README: &str = "claims/README.md";
const REGISTRY_PATH: &str = "aliases/subjects.yml";
/// The directory that marks a corpus root, and therefore the first refusal condition.
const CLAIMS_DIR: &str = "claims";
/// The registry's directory. It is a refusal condition **only when it is a link** — see
/// [`init_at`] — because a real one is under the root the caller named.
const ALIASES_DIR: &str = "aliases";

/// The registry's own file format and version. The version is provenance that rides the
/// claim's `canonical.subject_registry`; the aliases themselves are what the address covers,
/// so it is stated once here and written into both files from this constant.
const REGISTRY_SCHEMA: &str = "req.subject_registry/0";
const REGISTRY_V: u8 = 1;

/// The upstream issue tracker: printed, never written into the adopter's project — a tool
/// must not guess the forge a project uses.
const ISSUES_URL: &str = "https://git.xantu-barb.ts.net/fdx/req/issues";

/// Why a path is refused, because the operator's next move depends on which: an existing corpus
/// is edited in place, while a *link* is removed (or `init` is run where the corpus should live).
/// One message for both would tell the second operator to edit a file that need not exist — a
/// dangling link is refused too.
enum Refusal {
    /// Something is already at this path, and it is not this run's to overwrite.
    Exists,
    /// A link here would put the corpus where the caller did not name it.
    Symlink,
}

/// How `init` can fail. The refusal is a decision with a remedy; everything else is a question
/// this run could not settle.
enum Failure {
    Refused(String, Refusal),
    Unverifiable(String),
}

/// `req init [nothing]` — the handler `main.rs` calls **before** the corpus root is
/// resolved. `cli_root` is the `--root` the tool stripped from argv: for `init` it is a
/// usage error, and this is where it is answered.
pub fn run(args: &[String], cli_root: Option<&Path>) -> i32 {
    if let Some(root) = cli_root {
        eprintln!(
            "usage: req init takes no --root\n  `init` creates a corpus in the current \
             directory; run it there instead:\n    cd {} && req init",
            root.display()
        );
        return 2;
    }
    if let Some(arg) = args.first() {
        eprintln!("usage: req init\nunexpected argument: {arg}");
        return 2;
    }
    match init_at(Path::new(".")) {
        Ok(()) => {
            print_next_steps();
            0
        }
        Err(Failure::Refused(what, why)) => {
            match why {
                Refusal::Exists => eprintln!(
                    "refusing to initialise: {what} already exists — a corpus is never \
                     overwritten. Edit it in place, or run `init` in a different directory."
                ),
                Refusal::Symlink => eprintln!(
                    "refusing to initialise: {what} is a symlink — writing through it would \
                     put the corpus where this run was not asked to create one. Remove the \
                     link, or run `init` where the corpus should live."
                ),
            }
            1
        }
        Err(Failure::Unverifiable(why)) => {
            eprintln!("[UNVERIFIABLE] init: {why}");
            3
        }
    }
}

/// Create the corpus under `root`. The refusal is checked first and **only** here: the
/// question is what this directory already holds, not what an ancestor holds.
fn init_at(root: &Path) -> Result<(), Failure> {
    // The registry's **directory component** is checked first, because it can *shadow* the loop
    // below: `symlink_metadata` follows a link in intermediate components, so with
    // `aliases -> elsewhere` and a registry already in `elsewhere`, the loop resolves
    // `aliases/subjects.yml` *through* the link, finds that file and answers "already exists —
    // edit it in place" — a remedy pointing into a tree this run never named. The link is the
    // more fundamental fact and the one the operator has to act on.
    //
    // The check is also deliberately *narrower* than the loop: a real `aliases/` directory must
    // stay usable — it is under the root the caller named, and `init` writes its registry into it
    // as it always has. What is refused is a **link**, which would put the registry outside the
    // corpus this run was asked to create, and nothing downstream would notice: `Created::dir`
    // asks `exists()`, which follows the link and reports "nothing to create". A *dangling* link
    // is worse still — the refusal would be skipped, `create_dir_all` would then fail on the
    // link, and the outcome would be UNVERIFIABLE (3) instead of the refusal (1) whose message
    // says what to do about it.
    if std::fs::symlink_metadata(root.join(ALIASES_DIR))
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(Failure::Refused(ALIASES_DIR.to_string(), Refusal::Symlink));
    }

    for (path, label) in [
        (root.join(CLAIMS_DIR), CLAIMS_DIR),
        (root.join(REGISTRY_PATH), REGISTRY_PATH),
    ] {
        // `symlink_metadata`, not `exists()`: a broken symlink at one of these paths is a
        // thing that is there, and writing through it would be an overwrite of the
        // author's file. `exists()` follows the link and answers "nothing here".
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            // …and a *link* is refused for a different reason from a file or a directory, so it
            // gets a different remedy: a dangling link does not resolve, and "edit it in place"
            // would name a path that is not there. Every path this loop covers can be a link,
            // which is why the reason is read from the metadata rather than assumed from the
            // check above.
            let why = if meta.file_type().is_symlink() {
                Refusal::Symlink
            } else {
                Refusal::Exists
            };
            return Err(Failure::Refused(label.to_string(), why));
        }
    }

    let date = today_utc().map_err(Failure::Unverifiable)?;
    let mut created = Created::default();
    match build(root, &date, &mut created) {
        Ok(()) => Ok(()),
        Err(failure) => {
            // Nothing half-written: a surviving `aliases/subjects.yml` would make every
            // later `init` refuse, and a claim without its derived fields would be red.
            // A removal that itself fails is **reported**, never swallowed: the guarantee
            // ("nothing is left behind") holds only while the operator can see when it does
            // not, and a stale tree needs a human to remove it.
            let leftovers = created.abort();
            if !leftovers.is_empty() {
                eprintln!(
                    "[UNVERIFIABLE] init: could not remove what a failed init wrote — \
                     remove it by hand:\n  {}",
                    leftovers.join("\n  ")
                );
            }
            Err(failure)
        }
    }
}

/// Write the three files, deriving the claim's identity through the real path and reading
/// the result back through the loader every other command uses.
fn build(root: &Path, date: &str, created: &mut Created) -> Result<(), Failure> {
    created.dir(&root.join(ALIASES_DIR))?;
    created.file(&root.join(REGISTRY_PATH), &registry_template())?;

    // Read the registry back the way every reader does, so the starter claim's parse is
    // resolved against the aliases that are actually on disk rather than a second copy.
    let registry = load_registry(root).map_err(Failure::Unverifiable)?;

    let template = claim_template(registry.version, date);
    let claim: crate::model::ClaimFile = yaml_serde::from_str(&template).map_err(|e| {
        Failure::Unverifiable(format!("the starter claim template does not parse: {e}"))
    })?;

    // The parity invariant decides, exactly as it does for `--fill`: a template whose
    // authored slots disagree with its own statement is this module's bug, and shipping it
    // unfilled would ship a claim the adopter cannot edit without a refusal.
    let verdict = evaluate_claim(&claim, &registry);
    if verdict.flags() > 0 {
        return Err(Failure::Unverifiable(format!(
            "the starter claim template disagrees with its own parse ({} flag(s)) — the \
             template's statement and slots are out of step",
            verdict.flags()
        )));
    }
    let form_json = canonical_json(&verdict.form);
    let address = address_hex(&form_json);
    let filled = fill_text(
        &template,
        &form_json,
        &address,
        RULES_V,
        registry.version as u32,
    )
    .map_err(Failure::Unverifiable)?;
    match verify_text(&filled, &form_json, RULES_V) {
        StoredState::Verified => {}
        StoredState::Candidate(why) | StoredState::Mismatch(why) => {
            return Err(Failure::Unverifiable(format!(
                "the filled starter claim does not verify: {why}"
            )));
        }
    }

    created.dir(&root.join(CLAIMS_DIR))?;
    created.file(&root.join(CLAIM_PATH), &filled)?;
    created.file(&root.join(CLAIMS_README), CLAIMS_README_TEMPLATE)?;

    // End to end, through the loader: `init` reports success only on the file set
    // `req canon --verify` reads, not on the text it intended to write.
    let registry = load_registry(root).map_err(Failure::Unverifiable)?;
    let claims = load_claims(root).map_err(Failure::Unverifiable)?;
    for loaded in &claims {
        let expected = canonical_json(&evaluate_claim(&loaded.claim, &registry).form);
        match verify_text(&loaded.text, &expected, RULES_V) {
            StoredState::Verified => {}
            StoredState::Candidate(why) | StoredState::Mismatch(why) => {
                return Err(Failure::Unverifiable(format!(
                    "{}: the corpus `init` wrote is not green: {why}",
                    loaded.path.display()
                )));
            }
        }
    }
    Ok(())
}

/// What `init` has put on disk so far — so a failure part-way leaves nothing behind, rather
/// than a partial corpus that is red *and* refuses the next `init`.
#[derive(Default)]
struct Created {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl Created {
    /// Create `path` (and its parents) only if it is not already there, remembering the
    /// one this run created so `abort` removes it and nothing else.
    fn dir(&mut self, path: &Path) -> Result<(), Failure> {
        if !path.exists() {
            std::fs::create_dir_all(path)
                .map_err(|e| Failure::Unverifiable(format!("create {}: {e}", path.display())))?;
            self.dirs.push(path.to_path_buf());
        }
        Ok(())
    }

    /// Write `path` **exclusively**, and remember it.
    ///
    /// `create_new` is the guard the refusal check cannot be: that check runs before the
    /// writes, so a `claims/REQ-0001.yml` — or a symlink at that name pointing outside the
    /// corpus — planted between the two (a concurrent `init`, an author's stray link) would
    /// be written through by `std::fs::write`, which follows symlinks and truncates. Here the
    /// write itself refuses an existing path, and the failure is UNVERIFIABLE rather than an
    /// overwrite. `baseline.rs` makes the same exclusive-creation guarantee for the same
    /// reason.
    fn file(&mut self, path: &Path, text: &str) -> Result<(), Failure> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| Failure::Unverifiable(format!("write {}: {e}", path.display())))?;
        file.write_all(text.as_bytes())
            .map_err(|e| Failure::Unverifiable(format!("write {}: {e}", path.display())))?;
        self.files.push(path.to_path_buf());
        Ok(())
    }

    /// Remove this run's files, then the directories it made, and **report** every removal
    /// that failed. A directory that is not empty (something the adopter put there) stays:
    /// `remove_dir` refuses, which is correct — but the caller must say so, because the
    /// guarantee this exists for ("a failed `init` leaves nothing behind") is false the
    /// moment a removal fails silently: the half-corpus would be red *and* would make the
    /// next `init` refuse, with nothing telling the operator to clean it up by hand.
    fn abort(&self) -> Vec<String> {
        let mut left_behind = Vec::new();
        for file in self.files.iter().rev() {
            if let Err(e) = std::fs::remove_file(file) {
                left_behind.push(format!("{}: {e}", file.display()));
            }
        }
        for dir in self.dirs.iter().rev() {
            if let Err(e) = std::fs::remove_dir(dir) {
                left_behind.push(format!("{}: {e}", dir.display()));
            }
        }
        left_behind
    }
}

/// The subject registry: the starter claim's statement resolves `the api` to `API`, and
/// without that alias the parse would flag the subject as an unregistered literal.
fn registry_template() -> String {
    format!(
        "# The subject registry: what the prose calls a thing, and the id its address covers.\n\
         # Alias sets must be disjoint — a surface that resolved to two subjects would be two\n\
         # different requirements sharing a name. An unregistered surface stays literal in the\n\
         # parse (never silently collapsed), so add an alias here before you rely on one.\n\
         schema: {REGISTRY_SCHEMA}\n\
         version: {REGISTRY_V}\n\
         subjects:\n\
         \x20 API:\n\
         \x20   label: The API\n\
         \x20   aliases:\n\
         \x20     - the api\n\
         \x20     - api\n"
    )
}

/// The claim file, with the two derived fields as **anchor lines** for [`fill_text`]: the
/// text below is what `init` hands the fill path, and what lands on disk is its output.
///
/// The anchor lines carry no inline comments: `fill_text` replaces those lines wholesale,
/// so a comment written there would vanish without a trace. The header comment does survive,
/// because it is a line of its own.
fn claim_template(registry_version: u8, date: &str) -> String {
    format!(
        "# The starter claim. Edit `statement`, then re-run `req canon --fill` — it rewrites the\n\
         # derived `canonical.form` and `address` below through this same derivation. Never\n\
         # hand-write either one. The authoring loop and its traps are in claims/README.md.\n\
         schema: {schema}\n\
         claim_id: REQ-0001\n\
         statement: \"The API shall respond to a health check within 200 ms.\"\n\
         modality: SHALL\n\
         subject: API\n\
         attribute: health_check_latency\n\
         operator: LE\n\
         value: \"0.2\"\n\
         unit: s\n\
         conditions: []\n\
         applicability: []\n\
         exception: null\n\
         temporal: null\n\
         quantifier: null\n\
         unstructured: false\n\
         canonical:\n\
         \x20 rules_v: {rules_v}\n\
         \x20 subject_registry: {registry_version}\n\
         \x20 form: \"\"\n\
         address: \"\"\n\
         rationale:\n\
         \x20 source: docs/requirements.md\n\
         \x20 date: {date}\n\
         \x20 rejected_alternatives: []\n\
         \x20 assumptions: []\n\
         relations: []\n",
        schema = claim_schema(),
        rules_v = RULES_V,
    )
}

/// The next steps, printed — never written: which CI file to add, the rule that makes it
/// correct, and where a req bug goes.
fn print_next_steps() {
    println!(
        "Wrote a working, verified corpus in the current directory:\n  \
         claims/REQ-0001.yml    the starter claim, already filled and verified\n  \
         claims/README.md       the authoring loop and its traps\n  \
         aliases/subjects.yml   the subject registry (add an alias before you rely on one)\n\
         \n\
         Next steps:\n\
         \x20 1. Edit `statement` — the slots below it are its authored parse — then re-run\n\
         \x20    `req canon --fill`. `req canon --verify` is the check that gates; `req canon`\n\
         \x20    only reports, and always exits 0.\n\
         \x20 2. Add .req/variants beside the corpus: one `key=value ...` gate invocation per\n\
         \x20    line, `#` comments, and a comment-only file while you declare no variants.\n\
         \x20    Every variant gets its own gate invocation — a claim the invocation leaves\n\
         \x20    undecided is not examined, so `gate` exits 3 (UNVERIFIABLE) and names it.\n\
         \x20    Add a variant here in the same commit that adds the `applicability:` that\n\
         \x20    needs it (ADR 0015: the invocation that decides every claim is the gate).\n\
         \x20 3. Add .github/workflows/req.yml (GitHub) or .forgejo/workflows/req.yml (Forgejo —\n\
         \x20    Forgejo reads .github/workflows/ only while no .forgejo/workflows/ exists):\n"
    );
    print!("{CI_SNIPPET}");
    println!(
        "\n\
         \x20    <PIN> is the merge SHA of the req commit you depend on — read it with\n\
         \x20    `git rev-parse --short origin/main`; req is deliberately unpublished\n\
         \x20    (publish = false), so the rev IS the provenance.\n\
         \n\
         \x20 4. Report a req bug at {ISSUES_URL} — include the statement, the command, its\n\
         \x20    exit code, and `req --version` (rules_v decides a verdict).\n"
    );
}

/// The CI snippet `init` prints. `docs/adopting.md` carries the same block inside its
/// "## CI" section — two copies of advice no adopter can afford to have drift, so the
/// equality is **asserted** (`the_printed_snippet_is_the_one_the_adopting_doc_carries`,
/// below) rather than left to review: the parts that matter (the `summary` fallback, the
/// `gate()` bookkeeping, `exit $rc`, the presence assertion) would otherwise be free to
/// diverge with nothing failing.
///
/// The presence assertion is not decoration: a **failed input redirection does not stop a
/// `set -e` script**, so without it an absent `.req/variants` silently skips every declared
/// variant and the step goes green. Measured, and the reason the assertion is explicit.
const CI_SNIPPET: &str = r#"name: req
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
      - name: req, installed at a pinned rev
        env:
          GIT_SSH_COMMAND: "ssh -o ControlMaster=no -o ControlPath=none -o StrictHostKeyChecking=accept-new"
        run: |
          set -eu
          CARGO_NET_GIT_FETCH_WITH_CLI=true cargo install \
            --git ssh://git@git.xantu-barb.ts.net/fdx/req.git \
            --rev <PIN> --root "$HOME/.local" --locked
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
          req --version
      - name: every stored address still recomputes (variant-independent)
        run: req canon --verify
      - name: the release gate — every declared variant, and the default when none is declared
        run: |
          set -eu
          # An absent variants file must be red. A failed INPUT REDIRECTION does not stop a
          # `set -e` script, so the file's presence is asserted rather than assumed —
          # otherwise every declared variant is silently skipped and the step still exits 0.
          [ -f .req/variants ] || { echo "missing .req/variants" >&2; exit 1; }
          # A job summary above the log (runner >= 13.2 sets one of these).
          # Absent variable = an older runner: the step must still pass, so the target is /dev/null.
          summary="${FORGEJO_STEP_SUMMARY:-${GITHUB_STEP_SUMMARY:-/dev/null}}"
          printf '## req gate\n\n| variant | exit | verdict |\n| --- | --- | --- |\n' >> "$summary"
          rc=0
          gate() {
            out="$(req gate "$@" 2>&1)" && code=0 || code=$?
            verdict="$(printf '%s' "$out" | sed -n 's/^verdict: //p' | head -1)"
            printf '| `%s` | %s | %s |\n' "${*:-<none>}" "$code" "${verdict:-no verdict line}" >> "$summary"
            [ "$code" -eq 0 ] || rc=$code
          }
          gated=0
          while IFS= read -r line; do
            variant="${line%%#*}"
            [ -n "$variant" ] || continue
            # unquoted on purpose: one line is several key=value arguments
            # shellcheck disable=SC2086
            gate $variant
            gated=$((gated + 1))
          done < .req/variants
          # A comment-only file declares one variant — the default — and the bare invocation is
          # its gate. With a line declared, the loop is the gate: a bare run beside it reports
          # UNVERIFIABLE on any corpus carrying a selector (ADR 0015), and a false red beside
          # green variants is how a real red gets ignored.
          if [ "$gated" -eq 0 ]; then
            gate
          fi
          exit $rc
"#;

/// `claims/README.md` — the authoring loop and its traps, in the adopter's words. Static
/// text: nothing here is derived from the corpus, so it cannot drift from what was written.
const CLAIMS_README_TEMPLATE: &str = r#"# claims/ — this project's requirement corpus

One claim per file, in the req claim-file format. The file name is the `claim_id` it
declares (`REQ-0001.yml`), the corpus is flat, and this README is not a claim (it is not
claim-shaped, so the loader leaves it alone).

A claim's structured slots are an **authored parse of its `statement`** — the parity
invariant. `req canon` re-parses the statement and reports every slot that disagrees;
`canonical.form` and `address` are *derived* from that parse, never authored.

## The authoring loop

1. Edit `statement` (and, when the statement really changed, the slots that parse it).
2. Re-run `req canon --fill`. It re-derives `canonical.form` and `address` and writes them
   into the anchor lines. **Never hand-write either one**: a hand-written digest is a
   critical finding, and `req canon --verify` is what detects it.
3. `req canon` reports the divergences and always exits 0. The check that gates is
   `req canon --verify`: exit 0 verified, 1 a digest that does not recompute, 3 a
   re-canonicalization candidate (unfilled, or filled under other rules) — re-fill, do not
   re-type.
4. Run `req gate` before you ship; the CI section of req's `docs/adopting.md` wires it.

## The traps

- **A numeric slot that disagrees with the parse is refused.** `req canon --fill` exits 3
  and writes nothing rather than enshrine the contradiction. Re-word the statement, or
  leave the claim unfilled — never edit the parse to fit the prose, and never the prose to
  fit the parse.
- **Values are canonical SI.** `within 200 ms` is `value: "0.2"`, `unit: s`: the exact
  decimal string the parser derives. `unit: ms` and `unit: seconds` are parse flags, and
  `"0.30"` is not a canonical decimal (`"0.3"` is).
- **An empty `claims/` is UNVERIFIABLE, not a pass.** Zero claims cannot be told from every
  claim having vanished, so `req canon --verify` and `req gate` exit 3. Never "clean up" to
  zero claims — delete a requirement by deleting its claim deliberately, in a commit that
  says so.
- **One obligation per claim.** A statement carrying a second modal (`shall … and shall
  not …`) is two claims; split it.
- **`applicability:` makes a claim variant-scoped.** Gate every variant you declare (one
  `req gate key=value ...` invocation per line in `.req/variants`): a claim the invocation
  leaves undecided is not examined, so `req gate` exits 3 and names it — the invocation that
  decides every claim is the gate (ADR 0015).
"#;

/// Today's date in UTC as `YYYY-MM-DD`, from `SystemTime` and the civil-from-days
/// conversion below — std only, no new dependency, no locale-dependent formatting.
fn today_utc() -> Result<String, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since_epoch| date_from_unix_secs(since_epoch.as_secs() as i64))
        .map_err(|e| format!("the system clock is before the Unix epoch: {e}"))
}

/// `YYYY-MM-DD` for a Unix timestamp. `div_euclid` so a pre-epoch instant still lands on
/// the day that contains it rather than being rounded toward zero into the next one.
fn date_from_unix_secs(secs: i64) -> String {
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since 1970-01-01 → (year, month, day): Howard Hinnant's `civil_from_days`, whose
/// era arithmetic is exact for every `i64` and needs no table and no leap-second bookkeeping
/// (UTC dates here have no seconds at all). The tests below pin it against known epoch
/// dates, including a leap day and a century that is not a leap year.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01, so the leap day is the last day of the year and the
    // month arithmetic below is a single linear formula.
    let shifted = days + 719_468;
    // Floor division, because `/` truncates toward zero and the era is negative before
    // 0000-03-01.
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153; // [0, 11], March = 0
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known epoch dates as the oracle — not a second copy of the arithmetic. Each pair is
    /// a day number and the date it is, independently checkable (`date -u -r <days*86400>`).
    #[test]
    fn civil_from_days_matches_known_dates() {
        for (days, expected) in [
            (0i64, "1970-01-01"),
            (1, "1970-01-02"),
            (-1, "1969-12-31"),
            (-719_468, "0000-03-01"), // the algorithm's own epoch
            (10_957, "2000-01-01"),
            (11_016, "2000-02-29"), // a leap day in a leap century
            (18_628, "2021-01-01"),
            (19_782, "2024-02-29"),
            (20_454, "2026-01-01"),
            (47_541, "2100-03-01"), // 2100 is not a leap year: no 2100-02-29 exists
        ] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(
                format!("{year:04}-{month:02}-{day:02}"),
                expected,
                "day {days}"
            );
        }
    }

    #[test]
    fn date_from_unix_secs_is_the_utc_day() {
        assert_eq!(date_from_unix_secs(0), "1970-01-01");
        // The last second of the day and the first of the next one, so the boundary is the
        // day and not "just after midnight".
        assert_eq!(date_from_unix_secs(86_399), "1970-01-01");
        assert_eq!(date_from_unix_secs(86_400), "1970-01-02");
        // Before the epoch: rounded toward the day that contains it, never toward zero.
        assert_eq!(date_from_unix_secs(-1), "1969-12-31");
        assert_eq!(date_from_unix_secs(-86_400), "1969-12-31");
    }

    #[test]
    fn the_claim_template_parses_as_an_unfilled_claim() {
        // A cheap in-crate guard on the string `init` writes: it parses as a typed
        // `ClaimFile`, its anchors are empty, and its declared versions come from the
        // constants. The end-to-end behaviour — the registry, the fill and the gate — is
        // driven through the CLI in `tests/init.rs`; this one fails with a message about
        // the template rather than about a subprocess.
        let date = "2026-01-01";
        let text = claim_template(REGISTRY_V, date);
        let claim: crate::model::ClaimFile =
            yaml_serde::from_str(&text).expect("the starter claim template parses");
        assert_eq!(claim.claim_id, "REQ-0001");
        assert_eq!(claim.schema, claim_schema());
        assert_eq!(claim.canonical.rules_v, Some(RULES_V));
        assert_eq!(claim.canonical.subject_registry, REGISTRY_V as u32);
        assert_eq!(claim.rationale.date, date);
        assert_eq!(
            claim.canonical.form.as_deref(),
            Some(""),
            "form is an anchor"
        );
        assert_eq!(claim.address.as_deref(), Some(""), "address is an anchor");
    }

    /// The one **fenced YAML block** in a document, without the fence. Panics when the
    /// document has none or more than one, because "the first one" is exactly the assumption
    /// that lets a test compare the wrong block and pass.
    fn the_only_yaml_block(markdown: &str) -> &str {
        const OPEN: &str = "```yaml\n";
        let mut blocks = markdown.match_indices(OPEN);
        let first = blocks.next().expect("the document has a ```yaml block");
        assert!(
            blocks.next().is_none(),
            "the document must have exactly one ```yaml block; the snippet test compares *the* block"
        );
        let after = &markdown[first.0 + OPEN.len()..];
        let close = after
            .find("\n```")
            .expect("the ```yaml block is closed with ```");
        &after[..close + 1]
    }

    /// `init` prints the CI snippet; `docs/adopting.md` shows it. The two copies are the same
    /// block today, and this is what keeps them the same block: the parts an adopter cannot
    /// afford to lose — the `summary` fallback, the `gate()` bookkeeping, `exit $rc`, the
    /// `[ -f .req/variants ]` presence assertion — are not pinned anywhere else.
    #[test]
    fn the_printed_snippet_is_the_one_the_adopting_doc_carries() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/adopting.md");
        let doc = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reads {}: {e}", path.display()));
        assert_eq!(
            the_only_yaml_block(&doc),
            CI_SNIPPET,
            "the CI snippet `init` prints and the block in docs/adopting.md have drifted apart"
        );
    }

    /// `Created::abort` removes **the directories this run made** — not only its files — and
    /// this is the only case that says so. `tests/init.rs` cannot reach it: `claims` is a
    /// refusal condition, so a run that gets far enough to create a directory has already
    /// passed the check, and the remaining way to fail is a write. A directory left behind is
    /// not cosmetic — it is what makes the *next* `init` refuse a corpus whose files are gone.
    ///
    /// The second half is the other direction: a directory `init` did **not** create is the
    /// adopter's, and the abort must not take it.
    #[test]
    fn abort_removes_only_the_directories_this_run_created() {
        let dir = std::env::temp_dir().join(format!("req-created-abort-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("theirs"))
            .expect("pre-creates a directory we did not make");

        let mut created = Created::default();
        // `assert!` rather than `.expect()`: `Failure` deliberately has no `Debug`, and a test
        // is not a reason to give a production error type one.
        assert!(
            created.dir(&dir.join("theirs")).is_ok(),
            "a directory that is already there is a no-op"
        );
        assert!(
            created.dir(&dir.join("ours")).is_ok(),
            "creates the directory this run owns"
        );
        assert!(
            created
                .file(&dir.join("ours/registry.yml"), "registry\n")
                .is_ok(),
            "writes into it"
        );
        assert!(dir.join("ours/registry.yml").is_file());

        let left_behind = created.abort();
        assert!(
            left_behind.is_empty(),
            "abort reported leftovers: {left_behind:?}"
        );
        assert!(
            !dir.join("ours").exists(),
            "the directory this run created must be gone, or the next `init` refuses a corpus \
             whose files are already removed"
        );
        assert!(
            dir.join("theirs").exists(),
            "a directory this run did not create is the adopter's and must survive the abort"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

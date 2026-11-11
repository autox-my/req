//! `req check`: may this repository ship? The release lists, folded into one verdict (ADR 0023).
//!
//! A repository declares its release in two plain-text lists beside the corpus: `.req/variants`
//! (one complete release context per line, for `req gate`) and `.req/projections` (one committed
//! document per line, for `req render --check`). This command reads both, runs one step per line,
//! and folds the results with the gate's own rule — **Fail outranks Unverifiable outranks Pass**
//! ([`fold`]) — so the aggregation every adopter used to re-implement in shell is written once and
//! tested once.
//!
//! What it does not do is the point of it:
//!
//! * **A step is the command's exit.** A variant step calls the code `req gate` calls
//!   ([`crate::gate::evaluate_selection`]) and a projection step the code `req render --check`
//!   calls ([`crate::view::check_render_projection`]); no policy is copied and nothing is shelled out.
//! * **Absence is not silence.** No `.req/variants` is the default variant (one bare gate, which a
//!   corpus carrying a selector answers 3) and the output says so; a list that exists and cannot be
//!   read as a regular file is 3; a present list that yields nothing is a defect or the default
//!   variant, never "nothing to do".
//! * **It validates before it runs.** Both lists are read once into bytes and every line is checked
//!   before any step starts; a malformed line is exit 2 naming `<file>:<line>`, and nothing ran.
//! * **No flag narrows the steps.** `PASS` is printed only when every declared step ran and passed.

use crate::canon::RULES_V;
use crate::completeness::{self, COMPLETENESS, Findings, Policy};
use crate::format::{self, Format};
use crate::gate::{self, Evaluation, GateDocument, Outcome, TestEvidence};
use crate::select::Context;
use serde::Serialize;
use std::path::Path;

/// The document's schema string (ADR 0023 §6), versioned independently of `RULES_V` like every
/// command's (`format.rs`).
pub const SCHEMA_CHECK: &str = "req.check/1";

const USAGE: &str = "usage: req check [--test-report <file>]... [--format text|json]";

const VARIANTS: &str = ".req/variants";
const PROJECTIONS: &str = ".req/projections";

/// The fold over step exits, restated from `gate::exit_code` (which ranks policy outcomes, not step
/// exits, so the two cannot share one function) and pinned by the every-combination test below: 1 if any step is a defect, else 3 if any cannot be confirmed,
/// else 0. Every step exit is one of 0, 1, 3 (the caller normalises anything else to 1 before
/// folding), so this is a ranking and not a sum: a defect outranks an open question, which outranks
/// a pass (ADR 0015). An empty list folds to 0 only because it has nothing to fold; `req check`
/// never calls it with no steps.
pub fn fold(exits: &[i32]) -> i32 {
    if exits.contains(&1) {
        1
    } else if exits.contains(&3) {
        3
    } else {
        0
    }
}

/// One thing the run examined, with the exit it earned.
struct Step {
    /// `gate` (a variants line), `render` or `trace` (a projections path, by the generator its
    /// header names), `completeness` (the structural omissions, ADR 0039) or `list` (the list
    /// itself could not be used as a list).
    kind: &'static str,
    /// The variants line, or the path.
    label: String,
    exit: i32,
    /// Lines that make a non-pass actionable without rerunning the step.
    notes: Vec<String>,
    /// For a gate step that ran: the `req.gate/2` document `req gate --format json` prints for the
    /// same line.
    gate: Option<GateDocument>,
    /// For the completeness step that ran: its findings (ADR 0039).
    completeness: Option<Findings>,
}

impl Step {
    /// A step's exit is 0, 1 or 3. Anything else is a bug in `check`, and it is surfaced as a
    /// defect naming the step rather than folded into some other class.
    fn new(
        kind: &'static str,
        label: String,
        exit: i32,
        mut notes: Vec<String>,
        gate: Option<GateDocument>,
    ) -> Self {
        let exit = if matches!(exit, 0 | 1 | 3) {
            exit
        } else {
            notes.push(format!(
                "this step returned exit {exit}, which is outside {{0, 1, 3}}: a bug in `req \
                 check`, reported as a defect and never folded into anything else"
            ));
            1
        };
        Self {
            kind,
            label,
            exit,
            notes,
            gate,
            completeness: None,
        }
    }
}

/// A list on disk, read once.
enum Listing {
    /// The path does not exist (and is not a link, dangling or otherwise).
    Absent,
    /// The path exists and cannot be used as a list; the reason says why.
    Unreadable(String),
    /// The list's text.
    Text(String),
}

/// Read `<root>/<name>` into text, exactly once. The path is classified before it is opened so a
/// FIFO is never opened (which blocks forever with no writer): a link is followed with `metadata`,
/// and anything that is not a regular file — a directory, a FIFO, a device, a link to either — is
/// unreadable, not absent. Only a path that does not exist at all is absent.
fn read_list(root: &Path, name: &str) -> Listing {
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Listing::Absent,
        Err(error) => {
            return Listing::Unreadable(format!(
                "{name} cannot be examined: {error}; a pinned list that became unusable is not \
                 absent, and a skipped check is never a pass"
            ));
        }
    }
    match std::fs::metadata(&path) {
        Err(error) => Listing::Unreadable(format!(
            "{name} exists but cannot be followed ({error}), e.g. a dangling link: a pinned list \
             that became unusable is not absent, and a skipped check is never a pass"
        )),
        Ok(metadata) if !metadata.is_file() => Listing::Unreadable(format!(
            "{name} exists but is not a regular file (a directory, FIFO or device): a pinned list \
             that became unusable is not absent, and a skipped check is never a pass"
        )),
        Ok(_) => match std::fs::read(&path) {
            Err(error) => Listing::Unreadable(format!(
                "{name} exists but cannot be read ({error}): a pinned list that became unusable is \
                 not absent, and a skipped check is never a pass"
            )),
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Listing::Text(text),
                Err(_) => Listing::Unreadable(format!("{name} is not valid UTF-8 text")),
            },
        },
    }
}

/// The words of one list line, as `scripts/check.sh` reads it: a trailing CR is stripped, `#`
/// starts a comment, and the rest is split on blanks (space and tab) with **no** pathname
/// expansion — a `*` in a value is data.
fn words(line: &str) -> Vec<&str> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let line = line.split('#').next().unwrap_or("");
    line.split([' ', '\t'])
        .filter(|word| !word.is_empty())
        .collect()
}

/// One validated `.req/variants` line.
struct VariantLine {
    /// The words, joined by one space: the label the step carries.
    text: String,
    context: Context,
}

/// Validate and parse `.req/variants`. A line is `key=value` words, never an option; a word that
/// starts with `-`, lacks `=` or has an empty key is a malformed list, named `<file>:<line>`.
/// Every malformed line is reported, so one run names them all. A final line with no newline
/// counts (`split('\n')`, not a reader that wants a terminator).
fn parse_variants(text: &str) -> Result<Vec<VariantLine>, Vec<String>> {
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let tokens = words(raw);
        if tokens.is_empty() {
            continue;
        }
        let mut context = Context::new();
        let mut ok = true;
        for token in &tokens {
            match token.split_once('=') {
                Some((key, value)) if !key.is_empty() && !token.starts_with('-') => {
                    context.insert(key.to_string(), value.to_string());
                }
                _ => {
                    errors.push(format!(
                        "{VARIANTS}:{number}: `{token}` is not a `key=value` word (an option, a \
                         word with no `=` and an empty key are refused): {}",
                        tokens.join(" ")
                    ));
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            lines.push(VariantLine {
                text: tokens.join(" "),
                context,
            });
        }
    }
    if errors.is_empty() {
        Ok(lines)
    } else {
        Err(errors)
    }
}

/// A repo-relative path stays under the corpus root: no root or drive prefix and no `..`. The list
/// is repository content (a pull request can edit it), and `Path::join` replaces its base with an
/// absolute path, so an unchecked line would make `req check` read a file outside the repository.
fn stays_inside_the_root(path: &str) -> bool {
    use std::path::Component;
    Path::new(path)
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

/// Validate and parse `.req/projections`: exactly one path per line (a path with whitespace would
/// be silently truncated to its first word, so more than one word is named, not half-checked), and
/// not option-shaped (`render --check` refuses a path that starts with `-`).
fn parse_projections(text: &str) -> Result<Vec<String>, Vec<String>> {
    let mut paths = Vec::new();
    let mut errors = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let tokens = words(raw);
        match tokens.as_slice() {
            [] => {}
            [path] if !path.starts_with('-') && stays_inside_the_root(path) => {
                paths.push((*path).to_string());
            }
            [path] if !path.starts_with('-') => errors.push(format!(
                "{PROJECTIONS}:{number}: `{path}` leaves the corpus root: one repo-relative path \
                 per line, never an absolute path and never a `..` component"
            )),
            [path] => errors.push(format!(
                "{PROJECTIONS}:{number}: `{path}` is option-shaped: one repo-relative path per \
                 line, and `render --check` does not take a path that starts with `-`"
            )),
            many => errors.push(format!(
                "{PROJECTIONS}:{number}: more than one word (one repo-relative path per line): {}",
                many.join(" ")
            )),
        }
    }
    if errors.is_empty() {
        Ok(paths)
    } else {
        Err(errors)
    }
}

/// What the argv asked for.
struct Invocation {
    /// Every `--test-report`, in order; the union is what each gate step judges (ADR 0024 §2).
    test_reports: Vec<String>,
    format: Format,
}

fn parse_args(args: &[String]) -> Result<Invocation, i32> {
    let (format, args) = format::take_format(args, USAGE)?;
    let mut test_reports: Vec<String> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let value = if arg == "--test-report" {
            index += 1;
            match args.get(index) {
                Some(value) => value.as_str(),
                None => {
                    eprintln!("--test-report needs a value: {USAGE}");
                    return Err(2);
                }
            }
        } else if let Some(value) = arg.strip_prefix("--test-report=") {
            value
        } else {
            // No `key=value`, no `--only`, no `--out`: nothing narrows the steps, and an argument
            // that looks like it might is a usage error and never a silent subset.
            eprintln!(
                "{USAGE}\nunexpected argument {arg:?}: `req check` takes no selection and has no \
                 flag that narrows the steps"
            );
            return Err(2);
        };
        gate::take_report_path(value, USAGE, &mut test_reports)?;
        index += 1;
    }
    Ok(Invocation {
        test_reports,
        format,
    })
}

/// A policy's outcome is a note only when it is not a pass.
fn gate_notes(evaluation: &Evaluation) -> Vec<String> {
    let mut notes: Vec<String> = evaluation
        .results
        .iter()
        .filter(|result| result.outcome != Outcome::Pass)
        .map(|result| format!("{}: {}", result.name, result.outcome.as_str()))
        .collect();
    if !evaluation.undecided.is_empty() {
        notes.push(format!(
            "undecided: {} claim(s) not examined by this invocation",
            evaluation.undecided.len()
        ));
    }
    notes
}

/// One variant step: the gate's evaluation with the shared evidence.
fn variant_step(label: String, context: Context, root: &Path, evidence: TestEvidence<'_>) -> Step {
    match gate::evaluate_selection(context, root, evidence) {
        Ok(evaluation) => {
            let notes = if evaluation.code == 0 {
                Vec::new()
            } else {
                gate_notes(&evaluation)
            };
            Step::new(
                "gate",
                label,
                evaluation.code,
                notes,
                Some(evaluation.document),
            )
        }
        Err(reason) => Step::new(
            "gate",
            label,
            3,
            vec![format!("[UNVERIFIABLE] {reason}")],
            None,
        ),
    }
}

/// One projection step: `req render --check <path>`'s judgement (or `req trace --check`'s, for a
/// trace projection), with the path resolved against
/// the corpus root (the list is the repository's, so its paths are repo-relative wherever the
/// command is run from).
fn projection_step(path: &str, root: &Path) -> Step {
    let full = root.join(path);
    // The ONE departure from "a step's exit is the command's exit" (ADR 0023 §3-§4): a path the
    // repository lists that is gone, or is not a regular file, is a defect (1), where
    // `render --check` — which can only say it cannot read the file — answers 3. A listed
    // projection that vanished is silent decay (ADR 0018). Nothing that is not a regular file is
    // ever handed to the judgement: opening a FIFO with no writer blocks the gate forever.
    match full.metadata() {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Step::new(
                "render",
                path.to_string(),
                1,
                vec![format!(
                    "not a regular file: {path} is listed in {PROJECTIONS} but is not a regular \
                     file, so it cannot be a projection"
                )],
                None,
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Step::new(
                "render",
                path.to_string(),
                1,
                vec![format!(
                    "missing: {path} is listed in {PROJECTIONS} but does not exist (a projection \
                     that vanished is silent decay, ADR 0018)"
                )],
                None,
            );
        }
        Err(error) => {
            return Step::new(
                "render",
                path.to_string(),
                3,
                vec![format!("cannot examine {path}: {error}")],
                None,
            );
        }
    }
    let (kind, judged) = crate::view::check_listed_projection(&full, root);
    let shown = full.display().to_string();
    let notes: Vec<String> = if judged.code == 0 {
        Vec::new()
    } else {
        // The judgement names the file it was handed; show the path as listed, not the root.
        format!("{}{}", judged.stdout, judged.stderr)
            .replace(&shown, path)
            .lines()
            .map(str::to_string)
            .collect()
    };
    Step::new(kind, path.to_string(), judged.code, notes, None)
}

/// The completeness step (ADR 0039): structural omissions across the declared releases, the signed
/// Baselines and the claims. An unusable `.req/completeness` is a list step's 3, never a pass.
fn completeness_step(
    root: &Path,
    releases: &[Context],
    policy: &Policy,
    listing: &Listing,
) -> Step {
    if let Listing::Unreadable(reason) = listing {
        return Step::new(
            "list",
            COMPLETENESS.to_string(),
            3,
            vec![reason.clone()],
            None,
        );
    }
    match completeness::evaluate(root, releases, policy) {
        Ok(findings) => {
            let mut step = Step::new(
                "completeness",
                Findings::label(),
                findings.exit(),
                findings.notes(),
                None,
            );
            step.completeness = Some(findings);
            step
        }
        Err(reason) => Step::new(
            "completeness",
            Findings::label(),
            3,
            vec![format!("[UNVERIFIABLE] {reason}")],
            None,
        ),
    }
}

fn exit_word(exit: i32) -> &'static str {
    match exit {
        0 => "PASS",
        1 => "FAIL",
        _ => "UNVERIFIABLE",
    }
}

#[derive(Serialize)]
struct StepDocument<'a> {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<&'a str>,
    exit: i32,
    verdict: &'static str,
    /// Why a non-pass step is one, in the words the text view prints under it.
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    notes: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    gate: Option<&'a GateDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completeness: Option<&'a Findings>,
}

/// `req.check/1` (ADR 0023 §6): the folded verdict and, per step, its kind, its line or path, its
/// exit and, for a gate step, the whole `req.gate/2` document.
#[derive(Serialize)]
struct CheckDocument<'a> {
    schema: &'static str,
    generator: &'static str,
    rules_v: u32,
    verdict: &'static str,
    /// `false` when `.req/projections` is absent: nothing was pinned, so nothing was checked, and
    /// the document says so rather than leaving it to the absence of steps.
    projections_pinned: bool,
    steps: Vec<StepDocument<'a>>,
}

fn print_json(steps: &[Step], code: i32, projections_pinned: bool) {
    let document = CheckDocument {
        schema: SCHEMA_CHECK,
        generator: "check",
        rules_v: RULES_V,
        verdict: gate::verdict_word(code),
        projections_pinned,
        steps: steps
            .iter()
            .map(|step| StepDocument {
                kind: step.kind,
                line: (step.kind == "gate").then_some(step.label.as_str()),
                path: matches!(step.kind, "render" | "trace" | "list")
                    .then_some(step.label.as_str()),
                exit: step.exit,
                verdict: gate::verdict_word(step.exit),
                notes: &step.notes,
                gate: step.gate.as_ref(),
                completeness: step.completeness.as_ref(),
            })
            .collect(),
    };
    println!("{}", format::document(&document));
}

fn print_text(steps: &[Step], code: i32, projections_pinned: bool) {
    let variants = steps.iter().filter(|step| step.kind == "gate").count();
    let projections = steps
        .iter()
        .filter(|step| step.kind == "render" || step.kind == "trace")
        .count();
    let lists = steps.iter().filter(|step| step.kind == "list").count();
    let completeness = steps
        .iter()
        .filter(|step| step.kind == "completeness")
        .count();
    let mut header =
        format!("# req check: {variants} variant step(s), {projections} projection(s)");
    if completeness > 0 {
        header.push_str(&format!(", {completeness} completeness step"));
    }
    if lists > 0 {
        // A list that could not be read is a step too: the header agrees with the lines below it.
        header.push_str(&format!(", {lists} list step(s)"));
    }
    println!("{header}");
    for step in steps {
        println!(
            "[{}] {} {} (exit {})",
            exit_word(step.exit),
            step.kind,
            step.label,
            step.exit
        );
        for note in &step.notes {
            println!("    {note}");
        }
    }
    if !projections_pinned {
        println!("projections: none pinned (.req/projections absent; nothing to check)");
    }
    println!(
        "verdict: {}",
        match code {
            0 => "PASS — every declared step ran and passed",
            1 => "FAIL — a defect in at least one step (the [FAIL] lines above)",
            _ =>
                "UNVERIFIABLE — nothing is known to be wrong, but at least one step cannot be \
                  confirmed (the [UNVERIFIABLE] lines above)",
        }
    );
}

/// The label of the one step that stands for a list with no release in it.
const NO_FILE: &str = "(no .req/variants: the default variant)";
const NO_RELEASE: &str = "(.req/variants declares no release: the default variant)";

/// `req check [--test-report <file>]... [--format text|json]`
///
/// Exit: **1** if any step is a defect, else **3** if any step cannot be confirmed, else **0**;
/// **2** for a usage error or a malformed list, decided before any step runs.
pub fn run(args: &[String], root: &Path) -> i32 {
    let invocation = match parse_args(args) {
        Ok(invocation) => invocation,
        Err(code) => return code,
    };

    // Read both lists once, then validate both before anything runs. Everything below uses these
    // bytes; nothing is re-read between validation and use.
    let variants_listing = read_list(root, VARIANTS);
    let projections_listing = read_list(root, PROJECTIONS);
    let completeness_listing = read_list(root, COMPLETENESS);
    let variants = match &variants_listing {
        Listing::Text(text) => Some(parse_variants(text)),
        _ => None,
    };
    let projections = match &projections_listing {
        Listing::Text(text) => Some(parse_projections(text)),
        _ => None,
    };
    let mut malformed: Vec<String> = Vec::new();
    if let Some(Err(errors)) = &variants {
        malformed.extend(errors.iter().cloned());
    }
    if let Some(Err(errors)) = &projections {
        malformed.extend(errors.iter().cloned());
    }
    // `.req/completeness` names claim ids, so it is validated against the corpus. A corpus that
    // cannot be loaded leaves it unvalidated here; the completeness step then answers 3 with the
    // load error, and nothing is read as a pass.
    let mut policy = Policy::default();
    if let Listing::Text(text) = &completeness_listing
        && let Ok(known) = completeness::known_ids(root)
    {
        match completeness::parse_policy(text, &known) {
            Ok(parsed) => policy = parsed,
            Err(errors) => malformed.extend(errors),
        }
    }
    if !malformed.is_empty() {
        for error in &malformed {
            eprintln!("{error}");
        }
        eprintln!("{USAGE}\nnothing was gated: a malformed list is refused before any step runs");
        return 2;
    }

    let finish = |steps: Vec<Step>, projections_pinned: bool| -> i32 {
        let exits: Vec<i32> = steps.iter().map(|step| step.exit).collect();
        let code = fold(&exits);
        match invocation.format {
            Format::Json => print_json(&steps, code, projections_pinned),
            Format::Text => print_text(&steps, code, projections_pinned),
        }
        code
    };

    // A pinned variants list that cannot be used is exit 3 before any step: there is no release
    // to examine, and the default variant is not a substitute for a list that exists.
    if let Listing::Unreadable(reason) = &variants_listing {
        let step = Step::new("list", VARIANTS.to_string(), 3, vec![reason.clone()], None);
        return finish(
            vec![step],
            projections_listing_is_present(&projections_listing),
        );
    }

    let mut steps: Vec<Step> = Vec::new();

    // The reports are loaded once, as their union, and shared by every gate step.
    let loaded_report = gate::load_reports(&invocation.test_reports);
    let evidence = || gate::report_evidence(&loaded_report);

    let lines: Vec<VariantLine> = match variants {
        Some(Ok(lines)) => lines,
        _ => Vec::new(),
    };
    if lines.is_empty() {
        let label = match variants_listing {
            Listing::Absent => NO_FILE,
            _ => NO_RELEASE,
        };
        steps.push(variant_step(
            label.to_string(),
            Context::new(),
            root,
            evidence(),
        ));
    }
    let releases: Vec<Context> = if lines.is_empty() {
        vec![Context::new()]
    } else {
        lines.iter().map(|line| line.context.clone()).collect()
    };
    for line in lines {
        steps.push(variant_step(line.text, line.context, root, evidence()));
    }
    steps.push(completeness_step(
        root,
        &releases,
        &policy,
        &completeness_listing,
    ));

    let mut projections_pinned = true;
    match projections_listing {
        Listing::Absent => projections_pinned = false,
        Listing::Unreadable(reason) => {
            steps.push(Step::new(
                "list",
                PROJECTIONS.to_string(),
                3,
                vec![reason],
                None,
            ));
        }
        Listing::Text(_) => {
            let paths = match projections {
                Some(Ok(paths)) => paths,
                _ => Vec::new(),
            };
            if paths.is_empty() {
                steps.push(Step::new(
                    "list",
                    PROJECTIONS.to_string(),
                    1,
                    vec![format!(
                        "{PROJECTIONS} pins no projection: nothing was checked, and a skipped \
                         check is never a pass"
                    )],
                    None,
                ));
            }
            for path in paths {
                steps.push(projection_step(&path, root));
            }
        }
    }

    finish(steps, projections_pinned)
}

/// Whether `.req/projections` was a pinned list (anything but absent), for the document's flag in
/// the one early exit where the list is never opened.
fn projections_listing_is_present(listing: &Listing) -> bool {
    !matches!(listing, Listing::Absent)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rank an exit has in the fold, written independently of `fold`'s branches: a pass is the
    /// lowest, an open question above it, a defect on top.
    fn rank(exit: i32) -> usize {
        match exit {
            0 => 0,
            3 => 1,
            1 => 2,
            other => panic!("not a step exit: {other}"),
        }
    }

    fn exit_of(rank: usize) -> i32 {
        [0, 3, 1][rank]
    }

    /// Every combination of 0/1/3 for lengths 1–3 (and the empty list), against the ranking
    /// oracle: the fold is the highest-ranked exit, so a defect outranks everything, an
    /// unverifiable outranks only a pass, and order never matters.
    #[test]
    fn the_fold_is_the_highest_ranked_exit_for_every_combination() {
        const EXITS: [i32; 3] = [0, 1, 3];
        let mut cases: Vec<Vec<i32>> = vec![Vec::new()];
        for length in 1..=3usize {
            let mut combos: Vec<Vec<i32>> = vec![Vec::new()];
            for _ in 0..length {
                combos = combos
                    .into_iter()
                    .flat_map(|prefix| {
                        EXITS.iter().map(move |exit| {
                            let mut next = prefix.clone();
                            next.push(*exit);
                            next
                        })
                    })
                    .collect();
            }
            cases.extend(combos);
        }
        // 1 + 3 + 9 + 27 combinations: the table is complete, not sampled.
        assert_eq!(cases.len(), 40);
        for exits in cases {
            let expected = exits
                .iter()
                .map(|exit| rank(*exit))
                .max()
                .map_or(0, exit_of);
            assert_eq!(fold(&exits), expected, "fold({exits:?})");
        }
    }

    /// The named cases the spec and the ADR state in words, as a hand-written table.
    #[test]
    fn the_fold_pins_the_precedence_by_name() {
        assert_eq!(fold(&[]), 0);
        assert_eq!(fold(&[0, 0, 0]), 0);
        assert_eq!(fold(&[0, 3, 0]), 3, "an unverifiable step is never a pass");
        assert_eq!(fold(&[3, 1]), 1, "a defect outranks an open question");
        assert_eq!(fold(&[1, 3]), 1, "and the order does not matter");
        assert_eq!(fold(&[0, 1]), 1);
    }

    /// An exit outside {0,1,3} is a bug, surfaced as a defect naming the step.
    #[test]
    fn a_step_exit_outside_the_contract_is_a_defect_with_a_note() {
        let step = Step::new("gate", "x=1".to_string(), 2, Vec::new(), None);
        assert_eq!(step.exit, 1);
        assert!(
            step.notes[0].contains("outside {0, 1, 3}"),
            "{:?}",
            step.notes
        );
    }

    #[test]
    fn a_line_is_read_as_the_release_script_reads_it() {
        assert_eq!(words("a=1  b=2\t# c\r"), ["a=1", "b=2"]);
        assert_eq!(words("# only a comment"), Vec::<&str>::new());
        assert_eq!(words("k=*"), ["k=*"]);
        assert_eq!(words("  \r"), Vec::<&str>::new());
    }
}

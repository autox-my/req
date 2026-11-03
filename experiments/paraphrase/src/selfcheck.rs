//! `--self-check`: the pure half, table-driven with hand-computed expected values.

use crate::cli::load_all;
use crate::data::{GeneratorFile, Statements, validate_generator, validate_statements};
use crate::judge::{Admission, Exclusion, Verdict, admit};
use crate::protocol;
use crate::sampler::{Candidate, Haystack, draw, is_single_obligation};
use crate::score::{self, Inputs, Outcome, SplitDetail, rank_shapes};
use crate::shape::bucket;
use crate::stats::wilson;
use req::canon::CanonicalForm;
use req::model::Registry;
use std::path::PathBuf;

type Case = (&'static str, fn() -> Result<(), String>);

fn near(name: &str, got: f64, want: f64) -> Result<(), String> {
    if (got - want).abs() <= 5e-4 {
        Ok(())
    } else {
        Err(format!("{name}: got {got:.4}, want {want:.4}"))
    }
}

fn form(subject: &str, action: &str, qualifier: &str) -> CanonicalForm {
    CanonicalForm {
        subject: subject.into(),
        action: action.into(),
        qualifier: qualifier.into(),
        ..CanonicalForm::default()
    }
}

fn wilson_3_of_10() -> Result<(), String> {
    // Hand-computed: p=.3, n=10, z=1.959964 -> centre .35552, half-width .24770 -> [.1078, .6032].
    let i = wilson(3, 10).ok_or("None for n=10")?;
    near("lo", i.lo, 0.1078)?;
    near("hi", i.hi, 0.6032)
}

fn wilson_is_not_symmetric() -> Result<(), String> {
    // A Wald interval is p +- w; Wilson 3/10 is pulled toward the middle: hi-p .3032 vs p-lo .1922.
    let i = wilson(3, 10).ok_or("None for n=10")?;
    let (up, down) = (i.hi - 0.3, 0.3 - i.lo);
    if up - down > 0.05 {
        Ok(())
    } else {
        Err(format!(
            "upper arm {up:.4} not clearly longer than lower arm {down:.4}"
        ))
    }
}

fn wilson_all_and_none() -> Result<(), String> {
    // 0/15: lo is exactly 0, hi = z^2/(n+z^2) = 3.8415/18.8415 = .2039. 15/15 mirrors it.
    let z = wilson(0, 15).ok_or("None for 0/15")?;
    near("0/15 lo", z.lo, 0.0)?;
    near("0/15 hi", z.hi, 0.2039)?;
    let a = wilson(15, 15).ok_or("None for 15/15")?;
    near("15/15 lo", a.lo, 0.7961)?;
    near("15/15 hi", a.hi, 1.0)
}

fn wilson_empty_is_none() -> Result<(), String> {
    match wilson(0, 0) {
        None => Ok(()),
        Some(i) => Err(format!("0/0 must be None, got {i:?}")),
    }
}

fn bucket_qualifier_only() -> Result<(), String> {
    let a = form("the tool", "export report", "within 5 seconds");
    let b = form("the tool", "export report", "inside five seconds");
    expect("qualifier-only difference", bucket(&a, &b), "qualifier")
}

fn bucket_sorted_join() -> Result<(), String> {
    let a = form("the tool", "export report", "daily");
    let b = form("the system", "produce report", "daily");
    expect(
        "subject+action difference",
        bucket(&a, &b),
        "action+subject",
    )
}

fn bucket_structure_mismatch() -> Result<(), String> {
    let a = form("the tool", "export report", "");
    let b = CanonicalForm {
        unstructured: true,
        raw: "reports can be exported".into(),
        ..CanonicalForm::default()
    };
    expect("structured vs unstructured", bucket(&a, &b), "structure")
}

fn bucket_identical_is_unexplained() -> Result<(), String> {
    let a = form("the tool", "export report", "");
    expect("no differing field", bucket(&a, &a.clone()), "unexplained")
}

fn expect(what: &str, got: String, want: &str) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}, want {want:?}"))
    }
}

fn filter_table() -> Result<(), String> {
    use Verdict::*;
    let rows: Vec<(&str, Vec<Option<Verdict>>, Admission)> = vec![
        (
            "unanimous yes",
            vec![Some(Yes), Some(Yes)],
            Admission::Admitted,
        ),
        (
            "unsure is dropped",
            vec![Some(Yes), Some(Unsure)],
            Admission::Excluded(Exclusion::Unsure),
        ),
        (
            "no is dropped",
            vec![Some(Yes), Some(No)],
            Admission::Excluded(Exclusion::Rejected),
        ),
        (
            "no outranks unsure",
            vec![Some(Unsure), Some(No)],
            Admission::Excluded(Exclusion::Rejected),
        ),
        (
            "a missing verdict is unjudged",
            vec![Some(Yes), None],
            Admission::Excluded(Exclusion::Unjudged),
        ),
        (
            "no judge admits nothing",
            vec![],
            Admission::Excluded(Exclusion::Unjudged),
        ),
    ];
    for (name, verdicts, want) in rows {
        let got = admit(&verdicts);
        if got != want {
            return Err(format!("{name}: got {got:?}, want {want:?}"));
        }
    }
    Ok(())
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sampler_single_obligation() -> Result<(), String> {
    let long = format!("The system shall {}.", vec!["process"; 40].join(" "));
    let rows: Vec<(&str, String, bool)> = vec![
        (
            "one shall sentence",
            "The system shall respond to a search request within 2 seconds.".into(),
            true,
        ),
        (
            "one must sentence",
            "Customers must have a valid login id to enter into the site.".into(),
            true,
        ),
        (
            "five words is too short",
            "The system shall log it.".into(),
            false,
        ),
        (
            "two sentences",
            "The system shall start quickly. The system shall stop quickly.".into(),
            false,
        ),
        (
            "shall and must",
            "The tool shall export reports and the tool must archive them.".into(),
            false,
        ),
        (
            "should is not an obligation here",
            "The system should log events in the audit store.".into(),
            false,
        ),
        (
            "a colon list",
            "The tool shall support formats: PDF, HTML and plain text output.".into(),
            false,
        ),
        (
            "a source encoding artifact",
            "The system shall display the organization\\92s logo on screen.".into(),
            false,
        ),
        ("forty-three words is too long", long, false),
    ];
    for (name, text, want) in rows {
        if is_single_obligation(&text) != want {
            return Err(format!("{name}: expected {want}"));
        }
    }
    Ok(())
}

fn cand(id: &str, text: &str) -> Candidate {
    Candidate {
        pair_id: id.into(),
        text: text.into(),
        source: "t".into(),
    }
}

fn sampler_order_dedupe_tuned() -> Result<(), String> {
    // sha256 prefixes (hand-run `shasum -a 256`): c3 = 7c1c97df, c2 = 9c0abe51, c1 = d0f631ca,
    // so the draw order is c3, c2, c1.
    let a = "The system shall respond to a search request within 2 seconds.";
    let b = "Users shall be able to export a report as a PDF file.";
    let c = "The server shall log every failed login attempt in the audit store.";
    let cands = vec![cand("c1", c), cand("c2", b), cand("c3", a)];
    let ids = |o: &crate::sampler::Outcome| {
        o.statements
            .iter()
            .map(|s| s.id.clone())
            .collect::<Vec<_>>()
    };

    let o = draw(&cands, 3, &|_| None);
    if ids(&o) != ["c3", "c2", "c1"] {
        return Err(format!("order by sha256(pair id): got {:?}", ids(&o)));
    }
    let o = draw(&cands, 2, &|_| None);
    if ids(&o) != ["c3", "c2"] {
        return Err(format!("count caps the draw: got {:?}", ids(&o)));
    }
    // c3 quoted in a tuning file: skipped and named with its root; the draw moves on.
    let o = draw(&cands, 2, &|c| {
        (c.pair_id == "c3").then(|| "tests".to_string())
    });
    if ids(&o) != ["c2", "c1"] || o.excluded_tuned != [("c3".to_string(), "tests".to_string())] {
        return Err(format!(
            "tuned exclusion: got {:?} / {:?}",
            ids(&o),
            o.excluded_tuned
        ));
    }
    // c2 repeats c3's text (case and final period aside): the first by order keeps it.
    let dup = vec![cand("c1", c), cand("c2", &a.to_uppercase()), cand("c3", a)];
    let o = draw(&dup, 3, &|_| None);
    if ids(&o) != ["c3", "c1"] {
        return Err(format!("duplicate text: got {:?}", ids(&o)));
    }
    Ok(())
}

fn sampler_quoted_detection() -> Result<(), String> {
    let c = cand(
        "E-X-1",
        "The system shall respond to a search request within 2 seconds.",
    );
    let mut quoted8 = Haystack::default();
    quoted8.add("tests quote: the system shall respond to a search request, then more");
    let mut quoted7 = Haystack::default();
    quoted7.add("system shall respond to a search request");
    let mut by_id = Haystack::default();
    by_id.add("see pair e-x-1 for the failing case");
    let mut other = Haystack::default();
    other.add("an unrelated sentence about exporting reports as PDF files");
    let rows = [
        (
            "8 consecutive words (case/punctuation blind)",
            quoted8.hit(&c),
            true,
        ),
        ("7 consecutive words is not enough", quoted7.hit(&c), false),
        ("the pair id", by_id.hit(&c), true),
        ("unrelated text", other.hit(&c), false),
    ];
    for (name, got, want) in rows {
        if got != want {
            return Err(format!("{name}: got {got}, want {want}"));
        }
    }
    Ok(())
}

fn split(id: &str, shape: &str) -> SplitDetail {
    SplitDetail {
        item_id: id.into(),
        class: "both_structured",
        shape: shape.into(),
        statement: String::new(),
        restatement: String::new(),
        differences: Vec::new(),
        restatement_lints: Vec::new(),
    }
}

fn shapes_ranked() -> Result<(), String> {
    // Counts: subject 3, modality 2, qualifier 2, action 1 — so count order differs from name order.
    let splits = vec![
        split("s3/R1", "subject"),
        split("s1/R2", "subject"),
        split("s2/R1", "subject"),
        split("s2/R2", "qualifier"),
        split("s4/R1", "qualifier"),
        split("s5/R1", "modality"),
        split("s6/R1", "modality"),
        split("s1/R1", "action"),
    ];
    let rows = rank_shapes(&splits);
    let names: Vec<&str> = rows.iter().map(|r| r.shape.as_str()).collect();
    if names != ["subject", "modality", "qualifier", "action"] {
        return Err(format!("count desc, ties by name: got {names:?}"));
    }
    let ex: Vec<&str> = rows[0]
        .examples
        .iter()
        .map(|e| e.item_id.as_str())
        .collect();
    if ex != ["s1/R2", "s2/R1"] {
        return Err(format!("2 examples in item-id order: got {ex:?}"));
    }
    let many: Vec<SplitDetail> = (0..12)
        .map(|i| split(&format!("s/R{i}"), &format!("shape{i:02}")))
        .collect();
    if rank_shapes(&many).len() != 10 {
        return Err("only the top 10 shapes are kept".to_string());
    }
    Ok(())
}

fn protocol_is_pinned_and_blind() -> Result<(), String> {
    let doc = include_str!("../PROTOCOL.md");
    for (name, text) in [
        ("generator instruction", protocol::GENERATOR_INSTRUCTION),
        ("generator format", protocol::GENERATOR_FORMAT),
        ("judge instruction", protocol::JUDGE_INSTRUCTION),
    ] {
        if !doc.contains(text) {
            return Err(format!("PROTOCOL.md does not carry the {name} verbatim"));
        }
    }
    for (name, prompt) in [
        (
            "generator",
            protocol::generator_prompt("The system shall do a thing."),
        ),
        (
            "judge",
            protocol::judge_prompt("The system shall do a thing.", "A thing shall be done."),
        ),
    ] {
        let leaked = protocol::leaks(&prompt);
        if !leaked.is_empty() {
            return Err(format!(
                "the {name} prompt mentions the thing being measured: {leaked:?}"
            ));
        }
    }
    if protocol::leaks("restate it so its canonical form hashes the same").is_empty() {
        return Err("the leak detector did not fire on a leaking prompt".to_string());
    }
    Ok(())
}

fn fixture_dir() -> PathBuf {
    manifest().join("fixtures")
}

fn registry() -> Result<Registry, String> {
    Registry::load(
        &manifest()
            .join("../../aliases/subjects.yml")
            .to_string_lossy(),
    )
}

fn validators_reject() -> Result<(), String> {
    let l = load_all(&fixture_dir()).map_err(|e| format!("fixture does not validate: {e:?}"))?;
    let st: &Statements = &l.statements;
    if !validate_statements(st).is_empty() {
        return Err("fixture statements invalid".into());
    }
    let base: GeneratorFile = l.generators[0].0.clone();
    let stem = base.generator.id.clone();
    let check = |name: &str, g: &GeneratorFile, sha: &str, needle: &str| -> Result<(), String> {
        let (errs, _) = validate_generator(g, st, sha, &stem);
        if errs.iter().any(|e| e.contains(needle)) {
            Ok(())
        } else {
            Err(format!(
                "{name}: no error containing {needle:?} in {errs:?}"
            ))
        }
    };
    let (clean, uncovered) = validate_generator(&base, st, &l.statements_sha, &stem);
    if !clean.is_empty() || uncovered != 0 {
        return Err(format!(
            "the clean fixture generator reported {clean:?}, {uncovered} uncovered"
        ));
    }
    let mut g = base.clone();
    g.items[0].restatements.pop();
    check(
        "two restatements",
        &g,
        &l.statements_sha,
        "the protocol asks for 3",
    )?;
    let mut g = base.clone();
    g.items[0].restatements[0].text = st.statements[0].text.to_uppercase();
    check(
        "a copy",
        &g,
        &l.statements_sha,
        "is a copy of the statement",
    )?;
    let mut g = base.clone();
    g.items[0].alteration.element = "tone".into();
    check("a bad element", &g, &l.statements_sha, "alteration element")?;
    let mut g = base.clone();
    g.prompt = "Paraphrase the requirement and keep the canonical form".into();
    check(
        "another prompt",
        &g,
        &l.statements_sha,
        "not the protocol's generator prompt",
    )?;
    check("wrong sample hash", &base, "0000", "statements_sha256")?;
    let mut g = base.clone();
    g.items[1].statement_id = "no-such".into();
    check(
        "unknown statement",
        &g,
        &l.statements_sha,
        "not a statement in the sample",
    )?;
    let mut g = base.clone();
    g.items.pop();
    let (errs, uncovered) = validate_generator(&g, st, &l.statements_sha, &stem);
    if !errs.is_empty() || uncovered != 1 {
        return Err(format!(
            "a left-out statement is a coverage note, not an error: {errs:?}, {uncovered}"
        ));
    }
    Ok(())
}

/// The fixture end to end. Expected values were worked out by hand from the statements and the
/// judge files, not copied from the scorer's output: gen-a admits 7 of 9 (one `no`, one `unsure`;
/// judge-a shares gen-a's family and is not used) and 4 of them have a different address; gen-b
/// admits 6 of 9 (one `no`, one `unsure`, one verdict missing) with 4 different; gen-b's F-3/ALT
/// (`shall` -> `must`) is not an alteration and collapses.
fn fixture_end_to_end() -> Result<(), String> {
    let l = load_all(&fixture_dir()).map_err(|e| format!("fixture does not validate: {e:?}"))?;
    let reg = registry()?;
    let run = |gens: &[(GeneratorFile, String)]| {
        score::score(&Inputs {
            statements: &l.statements,
            statements_sha: &l.statements_sha,
            generators: gens,
            judges: &l.judges,
            registry: &reg,
        })
    };
    let rec = run(&l.generators);
    let by = |id: &str| {
        rec.generators
            .iter()
            .find(|g| g.generator.id == id)
            .ok_or(format!("no report for {id}"))
    };
    let (a, b) = (by("gen-a")?, by("gen-b")?);

    let ha = a.headline.as_ref().ok_or("gen-a has no headline")?;
    let ca = &a.coverage;
    if (
        ca.restatements_generated,
        ca.admitted,
        ca.excluded_rejected,
        ca.excluded_unsure,
        ca.excluded_unjudged,
    ) != (9, 7, 1, 1, 0)
    {
        return Err(format!("gen-a coverage: {ca:?}"));
    }
    if (ha.n, ha.false_splits) != (7, 4)
        || ha.false_split_rate != "0.5714"
        || ha.wilson95_lo != "0.2505"
        || ha.wilson95_hi != "0.8418"
    {
        return Err(format!("gen-a headline: {ha:?}"));
    }
    if a.judges_used != ["judge-g"] || a.judges_not_used_same_family != ["judge-a"] {
        return Err(format!(
            "gen-a panel: used {:?}, same-family {:?}",
            a.judges_used, a.judges_not_used_same_family
        ));
    }
    if ha.shapes[0].shape != "action" || ha.shapes[0].count != 2 {
        return Err(format!("gen-a top shape: {:?}", ha.shapes[0].shape));
    }
    if !a.alterations.false_merges.is_empty() {
        return Err("gen-a has no collapsing alteration".into());
    }

    let hb = b.headline.as_ref().ok_or("gen-b has no headline")?;
    let cb = &b.coverage;
    if (
        cb.restatements_generated,
        cb.admitted,
        cb.excluded_rejected,
        cb.excluded_unsure,
        cb.excluded_unjudged,
    ) != (9, 6, 1, 1, 1)
    {
        return Err(format!("gen-b coverage: {cb:?}"));
    }
    if (hb.n, hb.false_splits) != (6, 4) || hb.wilson95_lo != "0.3000" || hb.wilson95_hi != "0.9032"
    {
        return Err(format!("gen-b headline: {hb:?}"));
    }
    let merged: Vec<&str> = b
        .alterations
        .false_merges
        .iter()
        .map(|m| m.item_id.as_str())
        .collect();
    if merged != ["F-3/ALT"] {
        return Err(format!("gen-b false merges: {merged:?}"));
    }
    // Never pooled: the two rates differ and no third figure exists.
    if ha.false_split_rate == hb.false_split_rate || rec.generators.len() != 2 {
        return Err("generators were pooled or conflated".into());
    }
    if rec.verdict != Outcome::Fail {
        return Err(format!(
            "a collapsed alteration is FAIL, got {:?}",
            rec.verdict
        ));
    }
    // gen-a alone is clean: PASS.
    let only_a = run(&l.generators[..1]);
    if only_a.verdict != Outcome::Pass {
        return Err(format!("gen-a alone should PASS, got {:?}", only_a.verdict));
    }
    // With no judge from another family the generator is UNVERIFIABLE, not a rate over nothing.
    let none: Vec<_> = l
        .judges
        .iter()
        .filter(|j| j.judge.id == "judge-a")
        .cloned()
        .collect();
    let r = score::score(&Inputs {
        statements: &l.statements,
        statements_sha: &l.statements_sha,
        generators: &l.generators[..1],
        judges: &none,
        registry: &reg,
    });
    if r.verdict != Outcome::Unverifiable || r.generators[0].headline.is_some() {
        return Err(format!(
            "no eligible judge must be UNVERIFIABLE with no headline, got {:?}",
            r.verdict
        ));
    }
    // The record is content-named and byte-stable.
    let (bytes1, stem1) = score::record_bytes(&rec);
    let (bytes2, stem2) = score::record_bytes(&run(&l.generators));
    if bytes1 != bytes2 || stem1 != stem2 || !stem1.starts_with("paraphrase-rules-v") {
        return Err("two identical runs must produce the same bytes and the same name".into());
    }
    Ok(())
}

const CASES: &[Case] = &[
    ("wilson 3/10 = [0.1078, 0.6032]", wilson_3_of_10),
    ("wilson interval is asymmetric", wilson_is_not_symmetric),
    ("wilson 0/15 and 15/15 hit the bounds", wilson_all_and_none),
    ("wilson with n=0 is None", wilson_empty_is_none),
    ("bucket: qualifier-only difference", bucket_qualifier_only),
    ("bucket: fields sorted and joined", bucket_sorted_join),
    (
        "bucket: structured vs unstructured",
        bucket_structure_mismatch,
    ),
    (
        "bucket: identical forms are 'unexplained'",
        bucket_identical_is_unexplained,
    ),
    ("filter: unanimous yes only", filter_table),
    (
        "sampler: single-obligation predicate",
        sampler_single_obligation,
    ),
    (
        "sampler: sha256 order, dedupe, tuned exclusion",
        sampler_order_dedupe_tuned,
    ),
    (
        "sampler: quoted-in-tuning-files detection",
        sampler_quoted_detection,
    ),
    ("shapes: ranked, top 10, 2 examples", shapes_ranked),
    (
        "protocol: pinned in PROTOCOL.md and blind to Tier-1",
        protocol_is_pinned_and_blind,
    ),
    (
        "validators: reject what the protocol forbids",
        validators_reject,
    ),
    (
        "fixture: end to end against hand-computed counts",
        fixture_end_to_end,
    ),
];

/// Runs every case; returns the number that failed.
pub fn run() -> usize {
    let mut failed = 0;
    for (name, case) in CASES {
        match case() {
            Ok(()) => println!("PASS  {name}"),
            Err(why) => {
                failed += 1;
                println!("FAIL  {name}: {why}");
            }
        }
    }
    println!("self-check: {} case(s), {} failed", CASES.len(), failed);
    failed
}

//! G-1: PURE as a fourth source for the direction gold set (`direction-v4`).
//!
//! The generator (`tools/corpus/build_direction.py`) is process tooling, never on the address path, so the
//! seams are its command line and its written outputs:
//!
//! * the generator's own self-check, which holds the PURE sentence filter, the noise rules and the numeric
//!   labelling, with oracles written from English (a bound's side, a native prohibition's swapped sense);
//! * its refusals, which happen **before** any fetch: a `pdftotext` that is not the pinned version, and an
//!   archive that is not the pinned bytes;
//! * the committed corpus and its record, which are what `req bench` and `scripts/check.sh` read.
//!
//! Regenerating the corpus needs the network and the 33 MB archive, so no test does it: `scripts/check.sh`
//! never regenerates (AGENTS.md). The regeneration is shown in the commit that adds the corpus.

use req::model::Corpus;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn generator() -> PathBuf {
    root().join("tools/corpus/build_direction.py")
}

/// `python3` resolved from the test process's own `PATH`; its absence is a failure named here.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("python3 is required for tools/corpus/build_direction.py — install it first");
}

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "req-direction-pure-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn generate(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(python3())
        .arg("-I")
        .arg(generator())
        .args(args)
        .output()
        .expect("runs build_direction.py")
}

/// A stand-in for `pdftotext` that only answers `-v`, as the real one does (to stderr).
#[cfg(unix)]
fn fake_pdftotext(version: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = scratch_dir().join("pdftotext");
    fs::write(
        &path,
        format!("#!/bin/sh\necho 'pdftotext version {version}' >&2\n"),
    )
    .expect("write the fake pdftotext");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("make it executable");
    path
}

#[test]
fn the_generators_self_check_holds_the_pure_filter_the_noise_rules_and_the_labelling() {
    let out = generate(&["--self-check".as_ref()]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the self-check fails:\n{err}");
    assert!(
        err.contains("pure source self-check: ok"),
        "the self-check must run the PURE half and say so:\n{err}"
    );
}

#[test]
fn the_generators_self_check_holds_the_curated_exclusions_the_dedupe_and_the_real_pool_filter() {
    let out = generate(&["--self-check".as_ref()]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the self-check fails:\n{err}");
    assert!(
        err.contains("pure exclusion self-check: ok"),
        "the self-check must run the exclusion half (an excluded sentence emits no pair) and say so:\n{err}"
    );
}

#[cfg(unix)]
#[test]
fn a_pdftotext_that_is_not_the_pinned_version_is_refused_naming_both_versions() {
    let fake = fake_pdftotext("1.2.3");
    let out = generate(&[
        "--version".as_ref(),
        "4".as_ref(),
        "--pure-zip".as_ref(),
        "/nonexistent/requirements.zip".as_ref(),
        "--pdftotext".as_ref(),
        fake.as_os_str(),
    ]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a foreign pdftotext must not run:\n{err}"
    );
    assert!(
        err.contains("26.11.0") && err.contains("1.2.3"),
        "the refusal must name the pinned and the found version:\n{err}"
    );
    assert!(
        out.stdout.is_empty(),
        "nothing is written when the extractor is refused"
    );
}

#[cfg(unix)]
#[test]
fn an_archive_that_is_not_the_pinned_bytes_is_refused_naming_the_pinned_checksum() {
    let fake = fake_pdftotext("26.11.0");
    let zip = scratch_dir().join("requirements.zip");
    fs::write(&zip, b"not the archive").expect("write a stand-in archive");
    let out = generate(&[
        "--version".as_ref(),
        "4".as_ref(),
        "--pure-zip".as_ref(),
        zip.as_os_str(),
        "--pdftotext".as_ref(),
        fake.as_os_str(),
    ]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a foreign archive must not be read:\n{err}"
    );
    assert!(
        err.contains("bc319fe28619f6290badff328ca159dd"),
        "the refusal must name the pinned md5:\n{err}"
    );
    assert!(
        out.stdout.is_empty(),
        "nothing is written for a refused archive"
    );
}

fn corpus(name: &str) -> Corpus {
    Corpus::load(
        root()
            .join(format!("bench/corpus/{name}.yml"))
            .to_str()
            .expect("a utf-8 path"),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn direction_v4_is_every_v3_pair_plus_attributed_pure_pairs() {
    let v3 = corpus("direction-v3");
    let v4 = corpus("direction-v4");
    assert_eq!(v4.version, 4);
    // Every v3 pair, in order, unchanged: v4 only adds.
    for (old, new) in [
        (&v3.equivalent, &v4.equivalent),
        (&v3.not_equivalent, &v4.not_equivalent),
    ] {
        assert!(new.len() >= old.len());
        assert_eq!(
            &new[..old.len()],
            &old[..],
            "v4 must begin with v3's pairs, unchanged"
        );
    }
    let added = &v4.not_equivalent[v3.not_equivalent.len()..];
    assert!(!added.is_empty(), "v4 adds the PURE pairs");
    assert_eq!(
        v4.equivalent.len(),
        v3.equivalent.len(),
        "PURE adds no equivalent pair"
    );
    for pair in added {
        assert!(
            pair.id.starts_with("D-PU-"),
            "{}: a PURE pair id is `D-PU-<doc>-<n>…`",
            pair.id
        );
        assert!(
            pair.source.starts_with("pure@zenodo-1414117 ") && pair.source.contains(" #"),
            "{}: `source:` is `pure@zenodo-1414117 <file> #<n>`, got {:?}",
            pair.id,
            pair.source
        );
        assert!(pair.direction.is_some(), "{}: a direction label", pair.id);
    }
    // G-1-fix: the curated exclusions emit no pair, and the count of what was added is pinned.
    assert_eq!(added.len(), 834, "the PURE pairs of the committed corpus");
    for excluded in [
        "D-PU-1999-tcs-487-",
        "D-PU-2002-evla-back-162-",
        "D-PU-2004-rlcs-409-",
        "D-PU-2004-rlcs-410-",
        "D-PU-2004-rlcs-416-",
        "D-PU-2007-eirene-fun-7-507-",
        "D-PU-2007-ertms-229-",
        "D-PU-2010-home-1-3-146-",
        "D-PU-2007-nde-450-",
        "D-PU-2007-nde-572-",
        "D-PU-2004-rlcs-589-",
    ] {
        assert!(
            !added.iter().any(|p| p.id.starts_with(excluded)),
            "a domain-comparator, interleaved or duplicate statement must emit no pair: {excluded}"
        );
    }
    // Attribution (CC BY 4.0 needs it) and the extractor pin travel with the corpus.
    let rules = v4.rules.expect("the corpus header");
    for needed in [
        "Ferrari",
        "Spagnolo",
        "Gnesi",
        "PURE: A Dataset of Public Requirements Documents",
        "10.5281/zenodo.1414117",
        "CC BY 4.0",
        "pdftotext 26.11.0",
        "bc319fe28619f6290badff328ca159dd",
        "DOMAIN COMPARATOR",
        "8 by curation",
        "interleaved_columns 2",
        "probably labellable",
    ] {
        assert!(rules.contains(needed), "the header must carry `{needed}`");
    }
}

#[test]
fn the_v4_record_is_a_passing_run_with_no_dangerous_confusion_and_no_false_carry_on_the_admitted_class()
 {
    let text = fs::read_to_string(root().join("bench/results/direction-v4-rules-v9.json"))
        .expect("the v4 record is committed");
    let record: serde_json::Value = serde_json::from_str(&text).expect("the record is JSON");
    assert_eq!(record["corpus"], "bench/corpus/direction-v4.yml");
    assert_eq!(record["verdict"], "PASS");
    // Every number the docs quote (docs/canonical-hashing.md, *Recorded runs*; ADR 0040) is pinned here.
    assert_eq!(record["pairs_total"], 2661);
    assert_eq!(record["fmr"]["collapses"], 0);
    assert_eq!(record["fmr"]["not_equivalent_total"], 2382);
    assert_eq!(record["fmr"]["structurally_unverifiable_pairs"], 308);
    assert_eq!(record["fsr"]["splits"], 0);
    assert_eq!(record["fsr"]["structured_equivalent_total"], 199);
    assert_eq!(record["contradiction"]["false_contradictions"], 0);
    assert_eq!(record["contradiction"]["comparable"], 823);
    assert_eq!(record["contradiction"]["not_comparable"], 801);
    // Accuracy is pinned as the exact correct-over-classified pair, and the rate must be that ratio.
    assert_eq!(record["direction"]["checked"], 2353);
    assert_eq!(record["direction"]["correct"], 2299);
    let accuracy = record["direction"]["accuracy"].as_f64().expect("a rate");
    assert!(
        (accuracy - 2299.0 / 2353.0).abs() < 1e-12,
        "accuracy {accuracy} is not 2299/2353"
    );
    assert_eq!(
        record["direction"]["unclassifiable"]
            .as_array()
            .map(Vec::len),
        Some(308)
    );
    assert_eq!(
        record["direction"]["mismatches"].as_array().map(Vec::len),
        Some(54)
    );
    assert_eq!(
        record["direction"]["dangerous_confusions"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "a dangerous confusion is a hard stop"
    );
    let carry = &record["direction"]["false_carry"];
    assert_eq!(
        carry["modality_weakening"]["count"], 0,
        "a false carry on the admitted class is evidence against the admission"
    );
    // The exact false-carry numbers the record reports (G-1-fix review: "is a number" pinned nothing).
    for (class, support, admitted) in [
        ("modality_weakening", 271, true),
        ("numeric_loosening", 139, true),
        ("membership_widening", 0, false),
    ] {
        assert_eq!(carry[class]["count"], 0, "{class}");
        assert_eq!(carry[class]["support"], support, "{class}");
        assert_eq!(carry[class]["admitted"], admitted, "{class}");
    }
    // ADMITTED_CARRIES stays pinned to v2 (S-1/S-2): v4 measures, it does not admit. The record says
    // `admitted: true` for numeric_loosening, and the compiled set is still the v2 record's.
}

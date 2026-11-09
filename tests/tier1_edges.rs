//! Tier-1 canonicalization, edge by edge: what the first mutation run showed nothing was pinning
//! (`docs/research/mutation-testing.md`).
//!
//! Mutating `canon.rs` and running the suite left 126 mutants alive. Many sat on rules the corpora never
//! exercise, a word number above two, most of the unit table, a sign on a number, a control character in a statement,
//! a guard word inside another word, a bracket inside a list. A rule that no test pins is a rule a change can
//! break unnoticed, and in this crate a broken rule is a moved address.
//!
//! Every expectation here is written from the definition it tests (SI prefixes, the hashing spec's tables, the grammar of
//! ADR 0030), never copied from a run. Where a case showed behaviour the definition does not give, it is named in
//! the test, not hidden.

use req::canon::{
    CanonicalForm, canonical_decimal, canonical_json, canonicalize_statement, normalize_text,
};
use req::model::Registry;

fn derive(statement: &str) -> (CanonicalForm, Vec<String>) {
    let (form, lints) = canonicalize_statement(statement, &Registry::empty());
    (form, lints.into_iter().map(|l| l.code).collect())
}

fn value_unit(statement: &str) -> (Option<String>, Option<String>) {
    let (f, _) = derive(statement);
    (f.value.as_ref().map(canonical_decimal), f.unit.clone())
}

// ---------------------------------------------------------------------------------------------
// numbers: words, signs, separators, syntax
// ---------------------------------------------------------------------------------------------

#[test]
fn every_word_number_from_zero_to_ten_is_that_number() {
    for (word, n) in [
        ("zero", "0"),
        ("one", "1"),
        ("two", "2"),
        ("three", "3"),
        ("four", "4"),
        ("five", "5"),
        ("six", "6"),
        ("seven", "7"),
        ("eight", "8"),
        ("nine", "9"),
        ("ten", "10"),
    ] {
        let (f, lints) = derive(&format!("The system shall store at least {word} records."));
        assert_eq!(f.operator_str(), "GE", "{word}");
        assert_eq!(
            f.value.as_ref().map(canonical_decimal).as_deref(),
            Some(n),
            "{word}: {lints:?}"
        );
    }
}

#[test]
fn a_word_number_beyond_ten_is_not_a_number() {
    let (f, _) = derive("The system shall store at least eleven records.");
    assert_eq!(
        f.value, None,
        "the table is 0..=10 on purpose; eleven stays words"
    );
}

#[test]
fn each_unit_converts_by_its_si_factor_to_the_canonical_symbol() {
    // (unit as written, value of "1 <unit>" in the canonical unit, canonical unit). SI prefixes, not the code's table.
    let cases: &[(&str, &str, &str)] = &[
        ("s", "1", "s"),
        ("sec", "1", "s"),
        ("secs", "1", "s"),
        ("second", "1", "s"),
        ("seconds", "1", "s"),
        ("ms", "0.001", "s"),
        ("msec", "0.001", "s"),
        ("msecs", "0.001", "s"),
        ("millisecond", "0.001", "s"),
        ("milliseconds", "0.001", "s"),
        ("us", "0.000001", "s"),
        ("microsecond", "0.000001", "s"),
        ("microseconds", "0.000001", "s"),
        ("ns", "0.000000001", "s"),
        ("nanosecond", "0.000000001", "s"),
        ("nanoseconds", "0.000000001", "s"),
        ("hz", "1", "hz"),
        ("hertz", "1", "hz"),
        ("khz", "1000", "hz"),
        ("mhz", "1000000", "hz"),
        ("m", "1", "m"),
        ("meter", "1", "m"),
        ("metres", "1", "m"),
        ("mm", "0.001", "m"),
        ("millimeter", "0.001", "m"),
        ("millimeters", "0.001", "m"),
        ("cm", "0.01", "m"),
        ("centimeter", "0.01", "m"),
        ("centimeters", "0.01", "m"),
        ("km", "1000", "m"),
        ("kilometer", "1000", "m"),
        ("kilometers", "1000", "m"),
        ("n", "1", "n"),
        ("newton", "1", "n"),
        ("newtons", "1", "n"),
        ("kn", "1000", "n"),
        ("kilonewton", "1000", "n"),
        ("kilonewtons", "1000", "n"),
        ("a", "1", "a"),
        ("amp", "1", "a"),
        ("amps", "1", "a"),
        ("ampere", "1", "a"),
        ("amperes", "1", "a"),
        ("ma", "0.001", "a"),
        ("milliamp", "0.001", "a"),
        ("milliamps", "0.001", "a"),
        ("v", "1", "v"),
        ("volt", "1", "v"),
        ("volts", "1", "v"),
        ("mv", "0.001", "v"),
        ("millivolt", "0.001", "v"),
        ("millivolts", "0.001", "v"),
        ("w", "1", "w"),
        ("watt", "1", "w"),
        ("watts", "1", "w"),
        ("kw", "1000", "w"),
        ("kilowatt", "1000", "w"),
        ("kilowatts", "1000", "w"),
        ("j", "1", "j"),
        ("joule", "1", "j"),
        ("joules", "1", "j"),
        ("kj", "1000", "j"),
        ("kilojoule", "1000", "j"),
        ("kilojoules", "1000", "j"),
        ("k", "1", "k"),
        ("kelvin", "1", "k"),
        ("kelvins", "1", "k"),
        // Celsius is the one offset conversion: 1 degC is 274.15 K.
        ("c", "274.15", "k"),
        ("celsius", "274.15", "k"),
        // A percent is a hundredth, with no unit.
        ("percent", "0.01", "none"),
        ("pct", "0.01", "none"),
        ("%", "0.01", "none"),
    ];
    for (written, value, unit) in cases {
        let (v, u) = value_unit(&format!("The system shall draw at most 1 {written}."));
        assert_eq!(
            (v.as_deref(), u.as_deref()),
            (Some(*value), Some(*unit)),
            "1 {written}"
        );
    }
}

#[test]
fn units_that_are_not_decimally_convertible_stay_their_own_symbol_at_factor_one() {
    for unit in [
        "km/h", "m/s", "mph", "min", "hour", "day", "db", "rpm", "bar", "psi", "kpa", "byte",
        "kib", "records",
    ] {
        let (v, u) = value_unit(&format!("The system shall draw at most 3 {unit}."));
        assert_eq!(
            (v.as_deref(), u.as_deref()),
            (Some("3"), Some(unit)),
            "3 {unit}"
        );
    }
}

#[test]
fn a_sign_and_a_decimal_point_belong_to_the_number() {
    assert_eq!(
        value_unit("The system shall operate at least -10 celsius.")
            .0
            .as_deref(),
        Some("263.15"),
        "-10 degC is 263.15 K"
    );
    assert_eq!(
        value_unit("The system shall respond within 1.5 s.")
            .0
            .as_deref(),
        Some("1.5")
    );
    assert_eq!(
        value_unit("The system shall respond within +5 s.")
            .0
            .as_deref(),
        Some("5"),
        "an explicit plus sign"
    );
}

#[test]
fn a_unit_attached_to_the_number_is_the_unit() {
    assert_eq!(
        (
            value_unit("The system shall draw at most 2.5A.")
                .0
                .as_deref(),
            value_unit("The system shall draw at most 2.5A.")
                .1
                .as_deref()
        ),
        (Some("2.5"), Some("a"))
    );
    assert_eq!(
        value_unit("The system shall respond within 200ms."),
        (Some("0.2".to_owned()), Some("s".to_owned()))
    );
}

#[test]
fn a_range_or_a_malformed_number_is_not_guessed() {
    for statement in [
        "The system shall respond within 200-300 ms.",
        "The system shall respond within 1.2.3 ms.",
    ] {
        let (f, lints) = derive(statement);
        assert!(
            f.value.is_none() && f.unstructured,
            "{statement}: a range is two numbers, not one: {:?}",
            f
        );
        assert!(
            lints.contains(&"untestable_quantity".to_owned()),
            "{statement}: {lints:?}"
        );
    }
}

#[test]
fn a_digit_grouping_separator_inside_a_number_is_removed_and_nowhere_else() {
    for (written, normalized) in [
        ("1,000", "1000"),
        ("1_000", "1000"),
        ("1,2,3", "123"),
        ("12,345,678", "12345678"),
        ("a,1", "a,1"),
        ("1,a", "1,a"),
        ("1,", "1,"),
        (",1", ",1"),
        ("x_y", "x_y"),
        ("a, b", "a, b"),
    ] {
        assert_eq!(normalize_text(written), normalized, "{written:?}");
    }
    assert_eq!(
        value_unit("The module shall store at least 1,000 records.")
            .0
            .as_deref(),
        Some("1000")
    );
}

#[test]
fn an_upper_case_identifier_with_an_underscore_survives_normalization_and_other_words_are_lowercased()
 {
    for (word, normalized) in [
        ("REQ_TOOL", "REQ_TOOL"),
        ("Req_Tool", "Req_Tool"),
        ("NODE_ID-2", "NODE_ID-2"),
        ("Node_Id!", "node_id!"),
        ("NODE-ID", "node-id"),
        ("Mixed", "mixed"),
        ("tool's", "tool"),
    ] {
        assert_eq!(normalize_text(word), normalized, "{word:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// the units that are not in the table, and the words that are not units
// ---------------------------------------------------------------------------------------------

#[test]
fn a_short_unknown_unit_is_kept_verbatim_with_a_lint_and_a_long_word_is_a_qualifier() {
    let (f, lints) = derive("The system shall draw at most 5 zzzz.");
    assert_eq!(
        f.unit.as_deref(),
        Some("zzzz"),
        "four characters is still unit-shaped"
    );
    assert!(
        lints.contains(&"untestable_quantity".to_owned()),
        "{lints:?}"
    );
    let (f, lints) = derive("The system shall draw at most 5 zzzzz.");
    assert_eq!(f.unit, None, "five characters is a word, not a unit");
    assert_eq!(f.qualifier, "zzzzz");
    assert!(
        !lints.contains(&"untestable_quantity".to_owned()),
        "{lints:?}"
    );
}

#[test]
fn a_slash_or_percent_makes_an_unknown_token_unit_shaped_whatever_its_length() {
    let (f, lints) = derive("The system shall draw at most 5 widgets/minute.");
    assert_eq!(f.unit.as_deref(), Some("widgets/minute"));
    assert!(
        lints.contains(&"untestable_quantity".to_owned()),
        "{lints:?}"
    );
}

#[test]
fn a_prose_word_after_a_number_is_not_a_unit_even_when_it_is_short() {
    for word in [
        "on", "in", "of", "at", "each", "per", "for", "with", "and", "over",
    ] {
        let (f, _) = derive(&format!("The system shall draw at most 5 {word} track."));
        assert_eq!(f.unit, None, "5 {word}: {:?}", f);
        assert!(
            f.qualifier.starts_with(word),
            "the word stays in the qualifier, an identity field: {:?}",
            f.qualifier
        );
    }
}

#[test]
fn trailing_punctuation_does_not_change_which_unit_a_token_is() {
    assert_eq!(
        value_unit("The system shall draw at most 5 m/s."),
        (Some("5".to_owned()), Some("m/s".to_owned()))
    );
    assert_eq!(
        value_unit("The system shall draw at most 50%."),
        (Some("0.5".to_owned()), Some("none".to_owned()))
    );
}

// ---------------------------------------------------------------------------------------------
// the bytes
// ---------------------------------------------------------------------------------------------

#[test]
fn the_canonical_bytes_are_valid_json_and_round_trip_a_control_character_a_quote_and_a_backslash() {
    let statement = "The system shall log \u{1}x\"y\\z each command.";
    let (f, _) = derive(statement);
    let json = canonical_json(&f);
    let parsed: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("not JSON ({e}): {json}"));
    assert_eq!(
        parsed["action"], "log \u{1}x\"y\\z each command",
        "the action survives an escape round trip"
    );
    assert!(
        json.contains("\\u0001"),
        "a control character is escaped, not emitted raw: {json}"
    );
}

#[test]
fn the_documented_bytes_accessor_is_the_canonical_json() {
    let (f, _) = derive("The system shall respond within 200 ms.");
    assert_eq!(f.canonical_bytes(), canonical_json(&f));
    assert!(!f.canonical_bytes().is_empty());
}

// ---------------------------------------------------------------------------------------------
// guards: a condition is a whole word, and a chain has a bound
// ---------------------------------------------------------------------------------------------

#[test]
fn a_guard_word_inside_another_word_is_not_a_guard() {
    for statement in [
        "The system shall notify the shift manager.",
        "The system shall log whenever the brake is engaged.",
        "The system shall treat the elseif branch.",
        "The system shall log the ifs.",
    ] {
        let (f, _) = derive(statement);
        assert!(f.conditions.is_empty(), "{statement}: {:?}", f.conditions);
    }
}

#[test]
fn a_guard_with_nothing_after_it_is_not_a_condition() {
    let (f, _) = derive("The system shall log each command when.");
    assert!(f.conditions.is_empty(), "{:?}", f.conditions);
    assert!(
        f.action.ends_with("when"),
        "the word stays where it was: {:?}",
        f.action
    );
}

#[test]
fn a_chain_of_guards_is_extracted_from_the_end_and_sorted() {
    let (f, _) = derive("The system shall log each command when armed while idle.");
    let conditions: Vec<&str> = f.conditions.iter().map(String::as_str).collect();
    assert_eq!(conditions, ["when: armed", "while: idle"]);
    assert_eq!(f.action, "log each command");
}

#[test]
fn eight_guards_are_parsed_and_a_ninth_is_refused_with_a_lint() {
    let chain = |n: usize| -> String {
        let guards: Vec<String> = (1..=n).map(|i| format!("when g{i}")).collect();
        format!("The system shall log each command {}.", guards.join(" "))
    };
    let (f, lints) = derive(&chain(8));
    assert_eq!(f.conditions.len(), 8);
    assert!(
        !lints.contains(&"untestable_quantity".to_owned()),
        "eight is within the bound: {lints:?}"
    );
    let (_, lints) = derive(&chain(9));
    assert!(
        lints.contains(&"untestable_quantity".to_owned()),
        "a chain that long is left in raw: {lints:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// enumerations: the reader's edges (ADR 0030)
// ---------------------------------------------------------------------------------------------

fn members(statement: &str) -> Vec<String> {
    derive(statement).0.members.into_iter().collect()
}

fn set(statement: &str) -> (CanonicalForm, Vec<String>) {
    derive(statement)
}

#[test]
fn a_digit_count_in_the_header_is_checked_against_the_members_up_to_ninety_nine() {
    let (f, lints) =
        set("The report shall cite one of the 3 standards, ISO 9001, IEC 61508 or IEC 62304.");
    assert!(!f.unstructured && f.members.len() == 3, "{lints:?}");
    let (f, lints) =
        set("The report shall cite one of the 4 standards, ISO 9001, IEC 61508 or IEC 62304.");
    assert!(
        f.unstructured && lints.contains(&"membership_count_mismatch".to_owned()),
        "{lints:?}"
    );
    // 100 is not a count (three digits): the phrase is no header, so it reads as one more member of a bare list.
    let (f, lints) =
        set("The report shall cite one of the 100 standards, ISO 9001, IEC 61508 or IEC 62304.");
    assert!(
        !f.unstructured && !lints.contains(&"membership_count_mismatch".to_owned()),
        "{lints:?}"
    );
    assert_eq!(f.members.len(), 4, "{:?}", f.members);
    assert!(f.members.contains("the 100 standards"));
    // 99 still is one, and so is the largest two-digit count that matches.
    let (f, lints) =
        set("The report shall cite one of the 99 standards, ISO 9001, IEC 61508 or IEC 62304.");
    assert!(
        f.unstructured && lints.contains(&"membership_count_mismatch".to_owned()),
        "{lints:?}"
    );
    // "ab" is two characters but not digits: no count, so again a member.
    let (f, _) =
        set("The report shall cite one of the ab standards, ISO 9001, IEC 61508 or IEC 62304.");
    assert_eq!(f.members.len(), 4, "{:?}", f.members);
}

#[test]
fn a_header_is_recognised_by_a_count_or_following_and_never_by_the_article_alone() {
    let a =
        members("The report shall cite one of the following: ISO 9001, IEC 61508 or IEC 62304.");
    let b = members("The report shall cite one of following: ISO 9001, IEC 61508 or IEC 62304.");
    let c =
        members("The report shall cite one of three standards, ISO 9001, IEC 61508 or IEC 62304.");
    let expected = ["iec 61508", "iec 62304", "iso 9001"];
    assert_eq!(a, expected);
    assert_eq!(b, expected);
    assert_eq!(c, expected);
    // "the cat" is a member, not a header: the article alone introduces nothing.
    assert_eq!(
        members("The pet shall be one of the cat, the dog or the bird."),
        ["the bird", "the cat", "the dog"]
    );
}

#[test]
fn a_header_of_six_words_is_a_header_and_a_seventh_word_makes_it_a_member() {
    let six = members(
        "The report shall cite one of the three major international safety standards, ISO 9001, IEC 61508 or IEC 62304.",
    );
    assert_eq!(
        six,
        ["iec 61508", "iec 62304", "iso 9001"],
        "six words before the first comma"
    );
    let seven = members(
        "The report shall cite one of the three major international functional safety standards, ISO 9001, IEC 61508 or IEC 62304.",
    );
    assert_eq!(
        seven.len(),
        4,
        "seven words: no header, so the phrase is itself a member: {seven:?}"
    );
}

#[test]
fn a_colon_inside_a_token_or_inside_brackets_is_not_the_header_delimiter() {
    // 1500:2019 is a standard's number; its colon is followed by a digit.
    let m = members("The report shall cite one of ISO 1500:2019, IEC 61508 or IEC 62304.");
    assert_eq!(m, ["iec 61508", "iec 62304", "iso 1500:2019"]);
    // A colon inside parentheses belongs to the member.
    let m =
        members("The report shall cite one of ISO 9001 (note: current), IEC 61508 or IEC 62304.");
    assert_eq!(m, ["iec 61508", "iec 62304", "iso 9001 (note: current)"]);
    // A colon ending the text leaves nothing to list.
    let (f, lints) = set("The report shall cite one of the following:");
    assert!(
        f.unstructured && lints.contains(&"membership_unclear".to_owned()),
        "{lints:?}"
    );
}

#[test]
fn brackets_nest_and_a_comma_or_an_or_inside_one_does_not_split_a_member() {
    assert_eq!(
        members("The report shall cite one of {ISO 9001 (parts 1, 2), IEC 61508, IEC 62304}."),
        ["iec 61508", "iec 62304", "iso 9001 (parts 1, 2)"]
    );
    assert_eq!(
        members("The report shall cite one of ISO 9001 (part 1 or 2), IEC 61508 or IEC 62304."),
        ["iec 61508", "iec 62304", "iso 9001 (part 1 or 2)"]
    );
    assert_eq!(
        members("The report shall cite one of [ISO 9001, IEC 61508]."),
        ["iec 61508", "iso 9001"]
    );
    assert_eq!(
        members("The report shall cite one of (ISO 9001, IEC 61508)."),
        ["iec 61508", "iso 9001"]
    );
    let (f, lints) = set("The report shall cite one of {ISO 9001, IEC 61508.");
    assert!(
        f.unstructured && lints.contains(&"membership_unclear".to_owned()),
        "an unclosed bracket: {lints:?}"
    );
}

#[test]
fn a_serial_comma_before_or_and_a_second_or_in_one_item_are_told_apart() {
    assert_eq!(
        members("The report shall cite one of ISO 9001, IEC 61508, or IEC 62304."),
        ["iec 61508", "iec 62304", "iso 9001"]
    );
    let (f, lints) =
        set("The report shall cite one of ISO 9001, IEC 61508 or IEC 62304 or IEC 60601.");
    assert!(
        f.unstructured && lints.contains(&"membership_unclear".to_owned()),
        "two `or` in one item: {lints:?}"
    );
}

#[test]
fn what_follows_a_bracketed_list_is_the_tail_without_the_bracket() {
    let (f, _) = set("The report shall cite one of {ISO 9001, IEC 61508} and no other standard.");
    assert_eq!(f.qualifier, "and no other standard");
    assert_eq!(f.members.len(), 2);
}

#[test]
fn a_member_may_have_eight_words_outside_its_brackets_and_not_nine() {
    let eight = "alpha beta gamma delta epsilon zeta eta theta";
    let (f, lints) = set(&format!(
        "The report shall cite one of ISO 9001, {eight} or IEC 62304."
    ));
    assert!(!f.unstructured, "eight words is a name: {lints:?}");
    let (f, lints) = set(&format!(
        "The report shall cite one of ISO 9001, {eight} iota or IEC 62304."
    ));
    assert!(
        f.unstructured && lints.contains(&"membership_unclear".to_owned()),
        "nine words reads as a clause: {lints:?}"
    );
}

#[test]
fn a_one_followed_by_something_other_than_of_does_not_start_a_list() {
    let (f, _) = set("The report shall cite one standard of ISO 9001, IEC 61508 or IEC 62304.");
    assert_eq!(
        f.operator_str(),
        "none",
        "no `one of`, no set: {:?}",
        f.operator_str()
    );
    assert!(f.members.is_empty());
}

// ---------------------------------------------------------------------------------------------
// round two: what the rerun still showed alive
// ---------------------------------------------------------------------------------------------

#[test]
fn a_header_may_end_in_a_closing_bracket_before_its_colon() {
    // The colon after `)` is at depth 0 and ends the header; the bracketed note stays in the header.
    let (f, lints) = set(
        "The report shall cite one of the sources (see annex A): ISO 9001, IEC 61508 or IEC 62304.",
    );
    assert!(!f.unstructured, "{lints:?}");
    let m: Vec<&str> = f.members.iter().map(String::as_str).collect();
    assert_eq!(m, ["iec 61508", "iec 62304", "iso 9001"]);
}

#[test]
fn a_bracketed_aside_in_a_member_may_hold_connectives_and_many_words_but_the_name_outside_may_not()
{
    let m = members(
        "The report shall cite one of ISO 9001 (and later amendments), IEC 61508 or IEC 62304.",
    );
    assert_eq!(
        m,
        ["iec 61508", "iec 62304", "iso 9001 (and later amendments)"]
    );
    let long_aside = "ISO 9001 (one two three four five six seven eight nine ten eleven)";
    let m = members(&format!(
        "The report shall cite one of {long_aside}, IEC 61508 or IEC 62304."
    ));
    assert_eq!(m.len(), 3, "{m:?}");
    // The same words after the bracket are outside it and read as a clause.
    let (f, lints) =
        set("The report shall cite one of ISO 9001 (note) and more, IEC 61508 or IEC 62304.");
    assert!(
        f.unstructured && lints.contains(&"membership_unclear".to_owned()),
        "{lints:?}"
    );
}

#[test]
fn the_longest_obligation_phrase_at_the_earliest_place_decides_the_modality() {
    for (statement, modality) in [
        // `shall not exceed` is the comparison's negation (absorbed): the obligation survives as SHALL with LE.
        ("The system shall not exceed 5 ms.", "SHALL"),
        ("The system must not exceed 5 ms.", "SHALL"),
        ("The system shall not log secrets.", "SHALL-NOT"),
        ("The system must not log secrets.", "SHALL-NOT"),
        ("The system shall never log secrets.", "SHALL-NOT"),
        ("The system must never log secrets.", "SHALL-NOT"),
        ("The system shall log secrets.", "SHALL"),
        ("The system is required to log events.", "SHALL"),
        ("The system is to log events.", "SHALL"),
        ("The system should log events.", "SHOULD"),
        ("The system may log events.", "MAY"),
    ] {
        let (f, lints) = derive(statement);
        assert_eq!(f.modality_str(), modality, "{statement}: {lints:?}");
    }
}

#[test]
fn a_view_shows_the_derived_modality_or_the_word_unstructured() {
    use req::canon::derive as derive_claim;
    let claim = |statement: &str| -> req::model::ClaimFile {
        yaml_serde::from_str(&format!(
            "schema: req.claim/2\nclaim_id: REQ-0001\nstatement: {statement:?}\napplicability: []\nrelations: []\nrationale:\n  source: test\n  date: 2026-11-10\n  rejected_alternatives: []\n  assumptions: []\n"
        ))
        .unwrap_or_else(|e| panic!("claim does not parse: {e}"))
    };
    let reg = Registry::empty();
    assert_eq!(
        derive_claim(&claim("The system shall respond within 200 ms."), &reg).modality_label(),
        "SHALL"
    );
    assert_eq!(
        derive_claim(&claim("The system shall not log secrets."), &reg).modality_label(),
        "SHALL-NOT"
    );
    assert_eq!(
        derive_claim(&claim("The interface shall be friction-free."), &reg).modality_label(),
        "UNSTRUCTURED"
    );
}

// ---------------------------------------------------------------------------------------------
// round three
// ---------------------------------------------------------------------------------------------

#[test]
fn a_subject_that_is_a_quantity_is_not_a_subject() {
    // The text before the modal phrase is a value, not a thing: reading it as the subject would give the
    // claim someone else's identity ("5 ms" as a noun).
    for statement in [
        "5 ms shall be the limit.",
        "5 m/s shall be the limit.",
        "12kn shall be the limit.",
    ] {
        let (f, _) = derive(statement);
        assert!(f.unstructured, "{statement}: {:?}", f);
    }
}

#[test]
fn a_percent_sign_standing_alone_is_the_unit_and_surrounding_punctuation_is_not() {
    assert_eq!(
        value_unit("The system shall draw at most 5 % of capacity."),
        (Some("0.05".to_owned()), Some("none".to_owned()))
    );
    assert_eq!(
        value_unit("The system shall draw at most 5 (ms), measured."),
        (Some("0.005".to_owned()), Some("s".to_owned()))
    );
}

#[test]
fn of_two_guards_the_later_one_is_cut_first_wherever_the_words_fall() {
    let (f, _) = derive("The system shall log each command while idle when armed.");
    let conditions: Vec<&str> = f.conditions.iter().map(String::as_str).collect();
    assert_eq!(conditions, ["when: armed", "while: idle"]);
    assert_eq!(f.action, "log each command");
}

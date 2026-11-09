//! The tool-defect issue form (item 7 of the qualification-readiness package): a public place to
//! report that req gave a wrong or misleading answer, asking for what makes the report actionable.
//!
//! The form is YAML that GitHub renders; nothing here shows how GitHub renders it. What a test can
//! pin is what a careless edit would silently drop: the label that makes a report a *tool defect*, the
//! fields that make it reproducible (the whole `req --version`, the exact command, the exit code and
//! deciding line), the question that separates the dangerous failures (a requirement change read as no
//! change; a pass where nothing could be checked) from noise, the use-case question, the warning that
//! the repository is public, and the private route for vulnerabilities.

use serde_json::Value;
use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must exist: {e}"))
}

fn yaml(rel: &str) -> Value {
    yaml_serde::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel} must parse as YAML: {e}"))
}

fn field<'a>(form: &'a Value, id: &str) -> &'a Value {
    form["body"]
        .as_array()
        .expect("a body list")
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("the form has no field with id `{id}`"))
}

fn required(form: &Value, id: &str) -> bool {
    field(form, id)["validations"]["required"] == true
}

fn options(form: &Value, id: &str) -> Vec<String> {
    field(form, id)["attributes"]["options"]
        .as_array()
        .unwrap_or_else(|| panic!("`{id}` has options"))
        .iter()
        .map(|o| o.as_str().expect("a string option").to_owned())
        .collect()
}

const FORM: &str = ".github/ISSUE_TEMPLATE/tool-defect.yml";

#[test]
fn the_form_labels_a_report_as_a_tool_defect() {
    let form = yaml(FORM);
    assert!(
        form["name"].as_str().is_some_and(|n| !n.is_empty()),
        "a name"
    );
    assert!(
        form["description"].as_str().is_some_and(|d| !d.is_empty()),
        "a description"
    );
    let labels: Vec<&str> = form["labels"]
        .as_array()
        .expect("labels")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        labels,
        ["tool-defect"],
        "the maintainer adds `wrong-verdict` after triage, not the reporter"
    );
}

#[test]
fn the_fields_that_make_a_report_reproducible_are_required() {
    let form = yaml(FORM);
    for id in [
        "version",
        "statement_or_file",
        "command",
        "outcome",
        "expected",
    ] {
        assert!(required(&form, id), "`{id}` must be required");
    }
    let version = field(&form, "version")["attributes"]["label"]
        .as_str()
        .expect("a label");
    assert!(
        version.contains("req --version"),
        "the whole version output is asked for: {version}"
    );
}

#[test]
fn the_form_separates_the_dangerous_failures_from_noise() {
    let form = yaml(FORM);
    assert!(required(&form, "consequence"));
    let opts = options(&form, "consequence");
    assert!(
        opts.iter()
            .any(|o| o.contains("false merge") && o.contains("weakening")),
        "a requirement change read as no change is its own answer: {opts:?}"
    );
    assert!(
        opts.iter()
            .any(|o| o.contains("UNVERIFIABLE") && o.contains("pass")),
        "a pass where nothing could be checked is its own answer: {opts:?}"
    );
    assert!(
        opts.iter()
            .any(|o| o.contains("false split") || o.contains("noise")),
        "{opts:?}"
    );
}

#[test]
fn the_form_asks_which_use_the_report_affects() {
    let form = yaml(FORM);
    assert!(required(&form, "use_case"));
    let opts = options(&form, "use_case").join(" | ");
    for needle in ["meaning", "gate", "Baseline", "audit", "install"] {
        assert!(
            opts.contains(needle),
            "no use-case option mentions `{needle}`: {opts}"
        );
    }
}

#[test]
fn the_form_warns_that_the_repository_is_public() {
    let text = read(FORM);
    assert!(
        text.contains("public") && text.contains("confidential"),
        "a report must not carry a customer's requirement text into a public issue"
    );
    let form = yaml(FORM);
    let boxes = field(&form, "confirm")["attributes"]["options"]
        .as_array()
        .expect("checkboxes");
    assert!(
        boxes.iter().any(|b| b["required"] == true
            && b["label"]
                .as_str()
                .is_some_and(|l| l.contains("confidential"))),
        "the confidentiality box is required"
    );
}

#[test]
fn a_vulnerability_has_a_private_route_and_blank_issues_stay_open_for_other_things() {
    let config = yaml(".github/ISSUE_TEMPLATE/config.yml");
    assert_eq!(
        config["blank_issues_enabled"], true,
        "questions and feature requests are not tool defects"
    );
    let links = config["contact_links"].as_array().expect("contact links");
    assert!(
        links.iter().any(|l| l["url"]
            .as_str()
            .is_some_and(|u| u.contains("security/advisories/new"))),
        "the private vulnerability route is offered on the new-issue page"
    );
}

#[test]
fn the_docs_name_the_labels_and_the_form() {
    for (rel, needles) in [
        (
            "docs/adopting.md",
            vec!["Tool defect", "tool-defect", "wrong-verdict"],
        ),
        ("CONTRIBUTING.md", vec!["tool-defect", "Fixes #"]),
        (
            "docs/safety-manual.md",
            vec!["tool-defect", "wrong-verdict"],
        ),
    ] {
        let text = read(rel);
        for needle in needles {
            assert!(text.contains(needle), "{rel} must mention `{needle}`");
        }
    }
}

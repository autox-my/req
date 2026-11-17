//! Q-1 item 4 — `examples/propose-provider.sh` makes the `req propose` seam visible.
//!
//! The example is a deterministic stand-in (no model, no network); this test runs the real
//! binary against it and checks the documented answer: an unratified proposal, the provider
//! command recorded verbatim, nothing written.

use std::path::PathBuf;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn propose(a: &str, b: &str, provider: &str) -> (i32, String, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_req"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["propose", a, b, "--provider-cmd", provider])
        .output()
        .expect("runs the compiled binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn two_different_claims_get_an_unratified_related_proposal() {
    let provider = repo("examples/propose-provider.sh");
    let provider = provider.to_str().expect("a UTF-8 path");

    let (code, stdout, stderr) = propose("claims/REQ-0004.yml", "claims/REQ-0021.yml", provider);

    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        format!(
            "proposal (unratified): related (confidence 60%)\n  provider: cmd:{provider}\n  \
             nothing was written; a human ratifies\n"
        )
    );
}

#[test]
fn a_pair_that_differs_in_no_word_is_proposed_equivalent_and_still_unratified() {
    let provider = repo("examples/propose-provider.sh");
    let provider = provider.to_str().expect("a UTF-8 path");

    let (code, stdout, stderr) = propose("claims/REQ-0004.yml", "claims/REQ-0004.yml", provider);

    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.starts_with("proposal (unratified): equivalent (confidence 60%)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("provider: cmd:{provider}")),
        "{stdout}"
    );
}

#[test]
fn the_example_is_a_stand_in_that_reaches_for_no_model_and_no_network() {
    let text = std::fs::read_to_string(repo("examples/propose-provider.sh")).expect("the example");
    for forbidden in ["curl", "wget", "http://", "https://", "nc ", "ssh"] {
        assert!(
            !text.contains(forbidden),
            "the example mentions `{forbidden}`"
        );
    }
}

#[test]
fn the_adopting_page_shows_the_provider_and_says_it_is_a_stand_in() {
    let page = std::fs::read_to_string(repo("docs/adopting.md")).expect("the adopting page");
    assert!(
        page.contains("examples/propose-provider.sh"),
        "the page points at the example"
    );
    assert!(
        page.contains("process boundary"),
        "the page says a real provider is the adopter's own process boundary"
    );
}

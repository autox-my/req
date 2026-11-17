//! `claims/README.md` names the schema the claim files in this repository actually declare.
//!
//! The expected answer is read from the files, never typed here: a format bump that moves the
//! corpus but not its README fails this test, whichever way the numbers go.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn claims_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("claims")
}

/// Every `schema:` value declared by a `REQ-*.yml` file in `claims/`.
fn declared_schemas() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(claims_dir()).expect("claims/ is readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a claim file is readable");
        let value = text
            .lines()
            .find_map(|line| line.strip_prefix("schema:"))
            .unwrap_or_else(|| panic!("{} declares no `schema:` line", path.display()))
            .trim()
            .to_string();
        found.insert(value);
    }
    found
}

/// The schema the README's "Reading a file" section names: the `req.claim/<n>` that follows
/// "`schema` is the file-format version,".
fn schema_named_by_readme() -> String {
    let text = std::fs::read_to_string(claims_dir().join("README.md")).expect("README is readable");
    let anchor = "`schema` is the file-format version, `";
    let (_, rest) = text
        .split_once(anchor)
        .expect("the README states which schema the files use");
    let (named, _) = rest
        .split_once('`')
        .expect("the named schema is code-quoted");
    named.to_string()
}

#[test]
fn the_claims_readme_names_the_schema_every_claim_file_declares() {
    let declared = declared_schemas();
    assert_eq!(
        declared.len(),
        1,
        "the corpus declares one schema, or this README cannot name it: {declared:?}"
    );
    let named = schema_named_by_readme();
    assert!(
        declared.contains(&named),
        "claims/README.md names `{named}`, the files declare {declared:?}"
    );
}

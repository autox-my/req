//! `tools/reqif_import.py` — the ReqIF reader (issue #135, ADR 0028): a `.reqif` file in,
//! **legal, unratified** claim drafts out, and a report that accounts for every object in the file.
//!
//! The seam is the CLI, as in `tests/needs_import.rs`: the script is driven as a subprocess and then
//! the compiled `req` reads what it wrote. The three public files in `tests/fixtures/reqif/` are
//! read-only evidence; every expected value taken from them below is a **literal read from the pinned
//! bytes**, never computed by the script. A shape those files lack is a **synthetic** ReqIF the test
//! writes into a temporary directory (see `Doc`); no synthetic file is an export of any tool.
//!
//! The statements are read back through `req render`, so "a draft carries its statement" does not go
//! through the script's own quoting. `python3` is required: its absence is a **named failure**, never
//! a skip.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const ORIGIN: &str = "https://example.test/export/module.reqif";
const DATE: &str = "2026-11-05";

const POLARION: &str = "polarion-sample.reqif";
const DOORS: &str = "doors-sample.reqif";
const RMF: &str = "rmf-sample.reqif";

// Literals read from the pinned bytes of the three public files (tests/fixtures/reqif/README.md).
const POLARION_REQ_ID: &str = "rmf-1d312f18-fa7c-4de3-b8f3-ab105179fddf";
const POLARION_HEADING_ID: &str = "rmf-bd312f1z-fa7c-4de3-b8f3-ab105179fccc";
const POLARION_REQ_TEXT: &str = "The Lorem Ipsum shall do something.";
const POLARION_HEADING_TEXT: &str = "Section text...";
const DOORS_TYPE: &str = "_7f123ed4-98dd-4eed-b96a-edc8828963a8";
const DOORS_OBJECTS: [(&str, &str, &str); 3] = [
    (
        "_1_01ea51ea-ed54-471e-8c08-19e093f930f7",
        "1",
        "Carbon Trust Standard",
    ),
    (
        "_2_7e1c200d-5c01-4b93-941d-95fadf8feb0c",
        "2",
        "CRC and CCA",
    ),
    (
        "_3_a4285dbc-8259-4577-a093-ffd735084332",
        "3",
        "Picture or Whitepaper",
    ),
];
const RMF_TYPE: &str = "TC18xx SpecObjectType";
const RMF_TEXTS: [&str; 6] = ["Obj-01", "Obj-03", "Obj-06", "Obj-07", "Obj-08", "Obj-09"];

// --- the harness (the same shape as tests/needs_import.rs) ---------------------------------------

fn req_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_req")
        .map(PathBuf::from)
        .expect("integration tests run with CARGO_BIN_EXE_req set")
}

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/reqif_import.py")
}

/// `python3` resolved from the test process's own `PATH`, as an absolute path: the child's `PATH`
/// is rewritten (to put the built `req` on it), and a program name resolved through the rewritten
/// value would be a second, unintended dependency on it.
fn python3() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "python3 is required for tools/reqif_import.py — install it first \
         (macOS: `xcode-select --install`; Debian: `apt-get install python3`)"
    );
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn assert_code(&self, expected: i32, what: &str) {
        assert_eq!(
            self.code, expected,
            "{what}: expected exit {expected}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout, self.stderr
        );
    }

    /// The report's lines that start with `prefix` (a whole-line prefix, no indentation).
    fn lines(&self, prefix: &str) -> Vec<&str> {
        self.stdout
            .lines()
            .filter(|line| line.starts_with(prefix))
            .collect()
    }

    fn line(&self, prefix: &str) -> &str {
        let found = self.lines(prefix);
        assert_eq!(
            found.len(),
            1,
            "exactly one `{prefix}` line expected:\n{}",
            self.stdout
        );
        found[0]
    }
}

static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A base directory the suite controls, outside any checkout (see `tests/adopt.rs::fixture_base`).
fn fixture_base() -> PathBuf {
    let mut candidates = vec![std::env::temp_dir()];
    candidates.push(PathBuf::from("/tmp"));
    candidates.push(PathBuf::from("/var/tmp"));
    for base in candidates {
        if !base.is_dir() {
            continue;
        }
        let clean = !base
            .ancestors()
            .any(|dir| req::root::is_corpus_root(dir) || dir.join(".git").exists());
        if clean {
            return base;
        }
    }
    panic!(
        "no fixture base outside a checkout: TMPDIR, /tmp and /var/tmp all sit under claims/ or .git"
    );
}

/// A directory with no `claims/` and no `.git` ancestor, so the script's root resolution can only
/// find what the test put there.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = fixture_base().join(format!("{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates the fixture directory");
        if let Some(found) = dir.ancestors().find(|d| req::root::is_corpus_root(d)) {
            panic!(
                "the fixture assumes no ancestor holds claims/, but {} does",
                found.display()
            );
        }
        if let Some(found) = dir.ancestors().find(|d| d.join(".git").exists()) {
            panic!(
                "the fixture assumes it is outside a git repository, but {} holds .git",
                found.display()
            );
        }
        Fixture(dir)
    }

    /// A fixture with `req init` run in it and `module.reqif` holding `text`.
    fn with_reqif(name: &str, text: &str) -> Self {
        let fx = Fixture::new(name);
        req(fx.path(), &["init"]).assert_code(0, "req init");
        fx.write("module.reqif", text);
        fx
    }

    /// A fixture holding one of the three public files, byte for byte, as `module.reqif`.
    fn with_public(name: &str, file: &str) -> Self {
        let fx = Fixture::new(name);
        req(fx.path(), &["init"]).assert_code(0, "req init");
        let bytes = fs::read(public_path(file)).expect("reads the public fixture");
        fs::write(fx.path().join("module.reqif"), bytes).expect("copies the public fixture");
        fx
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("creates the fixture's parent directory");
        }
        fs::write(&path, text).unwrap_or_else(|e| panic!("writes {}: {e}", path.display()));
    }

    fn read(&self, rel: &str) -> String {
        let path = self.0.join(rel);
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reads {}: {e}", path.display()))
    }

    /// Every `claims/*.yml`, by name, with its exact bytes: a write is a new key, an overwrite a
    /// changed value, a refusal an equal map.
    fn claims(&self) -> BTreeMap<String, String> {
        let mut found = BTreeMap::new();
        for entry in fs::read_dir(self.0.join("claims")).expect("the claims directory") {
            let entry = entry.expect("a claim entry");
            if entry.path().extension().is_some_and(|e| e == "yml") {
                found.insert(
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read_to_string(entry.path()).expect("reads a claim"),
                );
            }
        }
        found
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn public_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reqif")
        .join(file)
}

fn req(cwd: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(req_bin())
        .args(args)
        .current_dir(cwd)
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs the compiled binary");
    outcome(&output)
}

fn outcome(output: &std::process::Output) -> Outcome {
    Outcome {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// How one run of the script is set up beyond its arguments.
#[derive(Default)]
struct Env<'a> {
    /// `REQ_ROOT`, when the run names its corpus that way.
    req_root: Option<&'a Path>,
    /// A directory placed **before** the built `req` on `PATH` (a fake `req`).
    path_first: Option<&'a Path>,
}

/// `tools/reqif_import.py`, run in `cwd` with the built `req` on `PATH`: the script takes the claim
/// schema and `rules_v` from `req --version`, never from a literal.
fn import_in(cwd: &Path, args: &[String], env: &Env) -> Outcome {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(first) = env.path_first {
        dirs.push(first.to_path_buf());
    }
    dirs.push(
        req_bin()
            .parent()
            .expect("the binary has a parent")
            .to_path_buf(),
    );
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = Command::new(python3());
    command
        .arg(script())
        .args(args)
        .current_dir(cwd)
        .env("PATH", std::env::join_paths(dirs).expect("a PATH"));
    match env.req_root {
        Some(root) => command.env("REQ_ROOT", root),
        None => command.env_remove("REQ_ROOT"),
    };
    outcome(&command.output().expect("runs tools/reqif_import.py"))
}

fn import(cwd: &Path, args: &[String]) -> Outcome {
    import_in(cwd, args, &Env::default())
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

/// `--from module.reqif --origin … --date …`, then `more`.
fn base(more: &[&str]) -> Vec<String> {
    let mut args = strings(&["--from", "module.reqif", "--origin", ORIGIN, "--date", DATE]);
    args.extend(strings(more));
    args
}

/// The arguments of a `--list` run.
fn list_args() -> Vec<String> {
    strings(&["--from", "module.reqif", "--list"])
}

fn without(args: Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
            continue;
        }
        if arg == flag {
            skip_value = true;
            continue;
        }
        out.push(arg);
    }
    out
}

// --- reading the report and the claims ----------------------------------------------------------

struct Draft {
    claim: String,
    identifier: String,
    human: String,
}

fn drafts_of(out: &Outcome) -> Vec<Draft> {
    out.lines("draft ")
        .into_iter()
        .map(|line| {
            let parts: Vec<&str> = line["draft ".len()..].split("  ").collect();
            assert_eq!(
                parts.len(),
                3,
                "a draft line is `draft <claim>  <IDENTIFIER>  <human id or ->`: {line:?}"
            );
            Draft {
                claim: parts[0].to_string(),
                identifier: parts[1].to_string(),
                human: parts[2].to_string(),
            }
        })
        .collect()
}

/// `skip <IDENTIFIER>  <reason>` lines, in report order, as `(identifier, reason)`.
fn skips_of(out: &Outcome) -> Vec<(String, String)> {
    let text = format!("{}\n{}", out.stdout, out.stderr);
    text.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("skip "))
        .map(|rest| {
            let (id, reason) = rest
                .split_once("  ")
                .expect("`skip <IDENTIFIER>  <reason>`");
            (id.to_string(), reason.to_string())
        })
        .collect()
}

struct Recon {
    total: usize,
    drafted: usize,
    skipped: usize,
    not_selected: usize,
    by_type: Option<String>,
}

fn number(text: &str) -> usize {
    text.trim()
        .parse()
        .unwrap_or_else(|_| panic!("a number, got {text:?}"))
}

/// `objects N = drafted D + skipped S + not selected M (by type: …)` from the report's last line (or
/// the message of a refusal that carries one).
fn recon_of(text: &str) -> Recon {
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("objects ") && line.contains(" = drafted "))
        .unwrap_or_else(|| panic!("a reconciling line is expected:\n{text}"));
    let rest = &line["objects ".len()..];
    let (total, rest) = rest.split_once(" = drafted ").expect("`= drafted`");
    let (drafted, rest) = rest.split_once(" + skipped ").expect("`+ skipped`");
    let (skipped, rest) = rest
        .split_once(" + not selected ")
        .expect("`+ not selected`");
    let (not_selected, by_type) = match rest.split_once(" (by type: ") {
        Some((count, tail)) => (count, Some(tail.trim_end_matches(')').to_string())),
        None => (rest, None),
    };
    Recon {
        total: number(total),
        drafted: number(drafted),
        skipped: number(skipped),
        not_selected: number(not_selected),
        by_type,
    }
}

fn assert_adds_up(out: &Outcome, what: &str) -> Recon {
    let recon = recon_of(&format!("{}\n{}", out.stdout, out.stderr));
    assert_eq!(
        recon.total,
        recon.drafted + recon.skipped + recon.not_selected,
        "{what}: the last line must add up"
    );
    recon
}

/// `req render` rows: claim id → (address, statement).
fn render_rows(cwd: &Path) -> BTreeMap<String, (String, String)> {
    let out = req(cwd, &["render"]);
    out.assert_code(0, "req render");
    let mut rows = BTreeMap::new();
    for line in out.stdout.lines() {
        let Some(rest) = line.strip_prefix("- REQ-") else {
            continue;
        };
        let parts: Vec<&str> = rest.splitn(4, "  ").collect();
        assert_eq!(
            parts.len(),
            4,
            "a render row is `- <id>  <address>  <modality>  <statement>`: {line:?}"
        );
        rows.insert(
            format!("REQ-{}", parts[0]),
            (parts[1].to_string(), parts[3].to_string()),
        );
    }
    rows
}

/// The statements of the drafts the run reported, in report order, read back through `req render`.
fn statements(fx: &Fixture, drafts: &[Draft]) -> Vec<String> {
    let rows = render_rows(fx.path());
    drafts
        .iter()
        .map(|draft| {
            rows.get(&draft.claim)
                .unwrap_or_else(|| panic!("{} is rendered", draft.claim))
                .1
                .clone()
        })
        .collect()
}

/// One claim file's field, by exact whole-line prefix.
fn field<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines().find(|line| line.starts_with(prefix))
}

fn claim_numbers(fx: &Fixture) -> Vec<String> {
    fx.claims().keys().cloned().collect()
}

// --- a synthetic ReqIF (never an export of a tool) -------------------------------------------------

const NS: &str = "http://www.omg.org/spec/ReqIF/20110401/reqif.xsd";

/// A **synthetic** ReqIF document the test writes: two object types (`Requirement`, `Note`), an
/// enumeration, and whatever objects, specifications and relations a test hands it. The shape is
/// invented for the test; it is not an export of DOORS, Polarion, ProR or any other tool.
struct Doc {
    objects: String,
    specs: String,
    relations: String,
    /// Extra attribute definitions appended to the `Requirement` type.
    extra_defs: String,
    created: String,
}

impl Doc {
    fn new() -> Self {
        Doc {
            objects: String::new(),
            specs: String::new(),
            relations: String::new(),
            extra_defs: String::new(),
            created: "1999-12-31T23:59:59Z".to_string(),
        }
    }

    fn object(mut self, xml: String) -> Self {
        self.objects.push_str(&xml);
        self
    }

    fn render(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- SYNTHETIC: written by tests/reqif_import.rs; not an export of any tool -->
<REQ-IF xmlns="{NS}" xmlns:xhtml="http://www.w3.org/1999/xhtml">
  <THE-HEADER>
    <REQ-IF-HEADER IDENTIFIER="H-1">
      <CREATION-TIME>{created}</CREATION-TIME>
      <REQ-IF-TOOL-ID>synthetic test writer</REQ-IF-TOOL-ID>
      <REQ-IF-VERSION>1.0</REQ-IF-VERSION>
      <TITLE>Synthetic</TITLE>
    </REQ-IF-HEADER>
  </THE-HEADER>
  <CORE-CONTENT>
    <REQ-IF-CONTENT>
      <DATATYPES>
        <DATATYPE-DEFINITION-STRING IDENTIFIER="DT-S" LONG-NAME="String"/>
        <DATATYPE-DEFINITION-XHTML IDENTIFIER="DT-X" LONG-NAME="Xhtml"/>
        <DATATYPE-DEFINITION-INTEGER IDENTIFIER="DT-I" LONG-NAME="Integer"/>
        <DATATYPE-DEFINITION-ENUMERATION IDENTIFIER="DT-E" LONG-NAME="Status values">
          <SPECIFIED-VALUES>
            <ENUM-VALUE IDENTIFIER="EV-DRAFT" LONG-NAME="Draft"/>
            <ENUM-VALUE IDENTIFIER="EV-APPROVED" LONG-NAME="Approved"/>
          </SPECIFIED-VALUES>
        </DATATYPE-DEFINITION-ENUMERATION>
      </DATATYPES>
      <SPEC-TYPES>
        <SPEC-OBJECT-TYPE IDENTIFIER="T-REQ" LONG-NAME="Requirement">
          <SPEC-ATTRIBUTES>
            <ATTRIBUTE-DEFINITION-XHTML IDENTIFIER="AD-TEXT" LONG-NAME="ReqIF.Text"><TYPE><DATATYPE-DEFINITION-XHTML-REF>DT-X</DATATYPE-DEFINITION-XHTML-REF></TYPE></ATTRIBUTE-DEFINITION-XHTML>
            <ATTRIBUTE-DEFINITION-STRING IDENTIFIER="AD-ID" LONG-NAME="ID"><TYPE><DATATYPE-DEFINITION-STRING-REF>DT-S</DATATYPE-DEFINITION-STRING-REF></TYPE></ATTRIBUTE-DEFINITION-STRING>
            <ATTRIBUTE-DEFINITION-INTEGER IDENTIFIER="AD-NUM" LONG-NAME="Num"><TYPE><DATATYPE-DEFINITION-INTEGER-REF>DT-I</DATATYPE-DEFINITION-INTEGER-REF></TYPE></ATTRIBUTE-DEFINITION-INTEGER>
            <ATTRIBUTE-DEFINITION-ENUMERATION IDENTIFIER="AD-STATUS" LONG-NAME="Status" MULTI-VALUED="true"><TYPE><DATATYPE-DEFINITION-ENUMERATION-REF>DT-E</DATATYPE-DEFINITION-ENUMERATION-REF></TYPE></ATTRIBUTE-DEFINITION-ENUMERATION>
            {extra}
          </SPEC-ATTRIBUTES>
        </SPEC-OBJECT-TYPE>
        <SPEC-OBJECT-TYPE IDENTIFIER="T-NOTE" LONG-NAME="Note">
          <SPEC-ATTRIBUTES>
            <ATTRIBUTE-DEFINITION-XHTML IDENTIFIER="AD-NTEXT" LONG-NAME="ReqIF.Text"><TYPE><DATATYPE-DEFINITION-XHTML-REF>DT-X</DATATYPE-DEFINITION-XHTML-REF></TYPE></ATTRIBUTE-DEFINITION-XHTML>
          </SPEC-ATTRIBUTES>
        </SPEC-OBJECT-TYPE>
        <SPEC-RELATION-TYPE IDENTIFIER="RT-1" LONG-NAME="Refines"/>
        <SPEC-RELATION-TYPE IDENTIFIER="RT-2" LONG-NAME="Verifies"/>
      </SPEC-TYPES>
      <SPEC-OBJECTS>
{objects}
      </SPEC-OBJECTS>
      <SPEC-RELATIONS>
{relations}
      </SPEC-RELATIONS>
      <SPECIFICATIONS>
{specs}
      </SPECIFICATIONS>
    </REQ-IF-CONTENT>
  </CORE-CONTENT>
</REQ-IF>
"#,
            created = self.created,
            extra = self.extra_defs,
            objects = self.objects,
            relations = self.relations,
            specs = self.specs,
        )
    }
}

/// An object of `ty` (`T-REQ` or `T-NOTE`) with the given value elements.
fn obj(id: &str, ty: &str, values: &[String]) -> String {
    format!(
        "<SPEC-OBJECT IDENTIFIER=\"{id}\"><TYPE><SPEC-OBJECT-TYPE-REF>{ty}</SPEC-OBJECT-TYPE-REF></TYPE><VALUES>{}</VALUES></SPEC-OBJECT>\n",
        values.concat()
    )
}

/// A requirement whose `ReqIF.Text` is the XHTML `inner` (put inside `<THE-VALUE>` as written).
fn req_xhtml(id: &str, inner: &str) -> String {
    obj(id, "T-REQ", &[text_xhtml("AD-TEXT", inner)])
}

fn text_xhtml(def: &str, inner: &str) -> String {
    format!(
        "<ATTRIBUTE-VALUE-XHTML><DEFINITION><ATTRIBUTE-DEFINITION-XHTML-REF>{def}</ATTRIBUTE-DEFINITION-XHTML-REF></DEFINITION><THE-VALUE>{inner}</THE-VALUE></ATTRIBUTE-VALUE-XHTML>"
    )
}

/// A plain `STRING` value for the definition `def`; `value` is written as an XML attribute value
/// (use `&#10;` for a newline).
fn text_string(def: &str, value: &str) -> String {
    format!(
        "<ATTRIBUTE-VALUE-STRING THE-VALUE=\"{value}\"><DEFINITION><ATTRIBUTE-DEFINITION-STRING-REF>{def}</ATTRIBUTE-DEFINITION-STRING-REF></DEFINITION></ATTRIBUTE-VALUE-STRING>"
    )
}

fn int_value(def: &str, value: &str) -> String {
    format!(
        "<ATTRIBUTE-VALUE-INTEGER THE-VALUE=\"{value}\"><DEFINITION><ATTRIBUTE-DEFINITION-INTEGER-REF>{def}</ATTRIBUTE-DEFINITION-INTEGER-REF></DEFINITION></ATTRIBUTE-VALUE-INTEGER>"
    )
}

fn enum_value(def: &str, refs: &[&str]) -> String {
    let refs: String = refs
        .iter()
        .map(|r| format!("<ENUM-VALUE-REF>{r}</ENUM-VALUE-REF>"))
        .collect();
    format!(
        "<ATTRIBUTE-VALUE-ENUMERATION><DEFINITION><ATTRIBUTE-DEFINITION-ENUMERATION-REF>{def}</ATTRIBUTE-DEFINITION-ENUMERATION-REF></DEFINITION><VALUES>{refs}</VALUES></ATTRIBUTE-VALUE-ENUMERATION>"
    )
}

fn div(text: &str) -> String {
    format!("<xhtml:div>{text}</xhtml:div>")
}

/// A specification named `name` whose hierarchy is `nodes` (already-built `<SPEC-HIERARCHY>`).
fn specification(name: &str, nodes: &str) -> String {
    format!(
        "<SPECIFICATION IDENTIFIER=\"SP-{name}\" LONG-NAME=\"{name}\"><CHILDREN>{nodes}</CHILDREN></SPECIFICATION>\n"
    )
}

fn node(object: &str, children: &str) -> String {
    let kids = if children.is_empty() {
        String::new()
    } else {
        format!("<CHILDREN>{children}</CHILDREN>")
    };
    format!(
        "<SPEC-HIERARCHY IDENTIFIER=\"HN-{object}\"><OBJECT><SPEC-OBJECT-REF>{object}</SPEC-OBJECT-REF></OBJECT>{kids}</SPEC-HIERARCHY>"
    )
}

fn relation(id: &str, ty: &str, source: &str, target: &str) -> String {
    format!(
        "<SPEC-RELATION IDENTIFIER=\"{id}\"><TYPE><SPEC-RELATION-TYPE-REF>{ty}</SPEC-RELATION-TYPE-REF></TYPE><SOURCE><SPEC-OBJECT-REF>{source}</SPEC-OBJECT-REF></SOURCE><TARGET><SPEC-OBJECT-REF>{target}</SPEC-OBJECT-REF></TARGET></SPEC-RELATION>\n"
    )
}

const REQ_ARGS: [&str; 4] = ["--type", "Requirement", "--text-attr", "ReqIF.Text"];

fn synthetic_fixture(name: &str, doc: &Doc) -> Fixture {
    Fixture::with_reqif(name, &doc.render())
}

// --- (a) Polarion --------------------------------------------------------------------------------

#[test]
fn polarion_one_requirement_is_drafted_with_its_text_and_provenance() {
    let fx = Fixture::with_public("req-reqif-polarion", POLARION);
    let out = import(
        fx.path(),
        &base(&[
            "--type",
            "System Requirement",
            "--text-attr",
            "ReqIF.Text",
            "--id-attr",
            "ReqIF.ForeignID",
            "--write",
        ]),
    );
    out.assert_code(0, "Polarion, System Requirement");
    let drafts = drafts_of(&out);
    assert_eq!(drafts.len(), 1, "{}", out.stdout);
    assert_eq!(drafts[0].identifier, POLARION_REQ_ID);
    assert_eq!(drafts[0].human, "LOREM-818");
    assert_eq!(
        statements(&fx, &drafts),
        vec![POLARION_REQ_TEXT.to_string()]
    );

    let claim = fx.read(&format!("claims/{}.yml", drafts[0].claim));
    assert_eq!(
        field(&claim, "  source: "),
        Some(format!("  source: \"{ORIGIN}#{POLARION_REQ_ID}\"").as_str()),
        "{claim}"
    );
    // The header comment names the file, the IDENTIFIER and the human id.
    let header: String = claim
        .lines()
        .take_while(|l| l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        header.contains("module.reqif")
            && header.contains(POLARION_REQ_ID)
            && header.contains("LOREM-818"),
        "{header}"
    );

    let recon = assert_adds_up(&out, "Polarion");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (2, 1, 0, 1)
    );
    assert_eq!(recon.by_type.as_deref(), Some("Heading 1"));
    assert_eq!(
        out.lines("objects ").last().copied(),
        out.stdout.lines().last(),
        "the reconciling line is the last line"
    );
    assert!(
        out.stdout
            .contains("objects 2 = drafted 1 + skipped 0 + not selected 1 (by type: Heading 1)"),
        "{}",
        out.stdout
    );

    let hierarchy = out.line("hierarchy");
    assert!(
        hierarchy.contains("1 of 1") && hierarchy.contains("Test Export"),
        "one drafted object sits under a parent, in the named specification: {hierarchy}"
    );

    // The loader and the renderer take the draft.
    req(fx.path(), &["canon"]).assert_code(0, "req canon");
    let render = req(fx.path(), &["render"]);
    render.assert_code(0, "req render");
    assert!(
        render.stdout.contains(POLARION_REQ_TEXT),
        "{}",
        render.stdout
    );
}

#[test]
fn polarion_both_types_draft_two_and_the_plain_string_is_taken_by_the_value_kind() {
    let fx = Fixture::with_public("req-reqif-polarion-two", POLARION);
    let out = import(
        fx.path(),
        &base(&[
            "--type",
            "Heading",
            "--type",
            "System Requirement",
            "--text-attr",
            "ReqIF.Text",
            "--write",
        ]),
    );
    out.assert_code(0, "Polarion, both types");
    let drafts = drafts_of(&out);
    assert_eq!(
        drafts
            .iter()
            .map(|d| d.identifier.as_str())
            .collect::<Vec<_>>(),
        vec![POLARION_HEADING_ID, POLARION_REQ_ID],
        "file order"
    );
    // The heading's value is a plain STRING under an attribute declared XHTML: the value element
    // says what it is.
    assert_eq!(
        statements(&fx, &drafts),
        vec![
            POLARION_HEADING_TEXT.to_string(),
            POLARION_REQ_TEXT.to_string()
        ]
    );
    assert_eq!(drafts[0].human, "-", "no --id-attr, no human id");
    let recon = assert_adds_up(&out, "Polarion, both");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (2, 2, 0, 0)
    );
}

#[test]
fn list_shows_the_header_the_specifications_and_each_type_with_its_attributes() {
    let fx = Fixture::with_public("req-reqif-polarion-list", POLARION);
    let before = fx.claims();
    let out = import(fx.path(), &list_args());
    out.assert_code(0, "--list on the Polarion file");
    let header = out.line("reqif:");
    for part in [
        "Polarion ReqIF Roundtrip",
        "2021-07-01T01:12:06.121Z",
        "1.0",
        "Test export",
    ] {
        assert!(
            header.contains(part),
            "the header line names {part}: {header}"
        );
    }
    assert_eq!(
        out.line("specification "),
        "specification Test Export  objects 2"
    );
    let types = out.lines("type ");
    assert_eq!(types.len(), 2, "{}", out.stdout);
    assert!(
        types.contains(&"type Heading  _820f7cab-e89d-4793-a093-15c66d554fa4  objects 1")
            && types.contains(
                &"type System Requirement  _21afe70a-ac89-48e0-b394-d1cba35c3537  objects 1"
            ),
        "{}",
        out.stdout
    );
    let indented: Vec<&str> = out
        .stdout
        .lines()
        .filter(|l| l.starts_with(' '))
        .map(str::trim)
        .collect();
    // Declared kind, and how many objects of the type carry a value.
    assert!(
        indented.contains(&"ReqIF.Text  XHTML  1 with a value"),
        "{indented:?}"
    );
    assert!(
        indented.contains(&"ReqIF.ForeignID  STRING  0 with a value")
            && indented.contains(&"ReqIF.ForeignID  STRING  1 with a value"),
        "{indented:?}"
    );
    assert!(
        indented.contains(&"Status  ENUMERATION  1 with a value"),
        "{indented:?}"
    );
    assert!(
        indented.contains(&"ReqIF.ForeignCreatedOn  DATE  1 with a value"),
        "{indented:?}"
    );
    assert_eq!(fx.claims(), before, "--list writes nothing");
}

// --- (b) the enumeration ----------------------------------------------------------------------------

#[test]
fn where_selects_on_a_resolved_enumeration_and_a_value_nobody_has_is_exit_1() {
    let fx = Fixture::with_public("req-reqif-enum", POLARION);
    let out = import(
        fx.path(),
        &base(&[
            "--where",
            "Status=Draft",
            "--text-attr",
            "ReqIF.Text",
            "--write",
        ]),
    );
    out.assert_code(0, "--where Status=Draft");
    let drafts = drafts_of(&out);
    assert_eq!(drafts.len(), 1, "{}", out.stdout);
    assert_eq!(drafts[0].identifier, POLARION_REQ_ID);
    let recon = assert_adds_up(&out, "Status=Draft");
    assert_eq!((recon.total, recon.drafted, recon.not_selected), (2, 1, 1));

    let fx = Fixture::with_public("req-reqif-enum-none", POLARION);
    let before = fx.claims();
    let out = import(
        fx.path(),
        &base(&[
            "--where",
            "Status=Approved",
            "--text-attr",
            "ReqIF.Text",
            "--write",
        ]),
    );
    out.assert_code(1, "--where Status=Approved");
    for value in ["Approved", "Draft", "Rejected", "Reviewed"] {
        assert!(
            out.stderr.contains(value),
            "the message lists the Status values: {value}\n{}",
            out.stderr
        );
    }
    assert!(
        out.stderr.contains("carried by objects: Draft"),
        "the values objects carry are named:\n{}",
        out.stderr
    );
    assert_eq!(fx.claims(), before, "nothing written");
}

// --- (c) DOORS: unnamed types ------------------------------------------------------------------------------------

#[test]
fn doors_types_have_no_name_and_are_selected_by_identifier() {
    let fx = Fixture::with_public("req-reqif-doors", DOORS);
    let listed = import(fx.path(), &list_args());
    listed.assert_code(0, "--list on the DOORS file");
    let types = listed.lines("type ");
    assert_eq!(types.len(), 14, "{}", listed.stdout);
    assert!(
        types
            .iter()
            .all(|line| line.starts_with("type (unnamed)  ")),
        "no DOORS type has a name: {types:?}"
    );
    assert!(
        types
            .iter()
            .any(|l| l.contains(DOORS_TYPE) && l.ends_with("objects 3")),
        "{types:?}"
    );
    assert_eq!(listed.lines("specification ").len(), 1);
    assert_eq!(
        listed.line("specification "),
        "specification AMR Standards Documents  objects 3"
    );
    assert!(listed.line("reqif:").contains("IBM Rational DOORS"));

    let before = fx.claims();
    let unnamed = import(
        fx.path(),
        &base(&["--type", "(unnamed)", "--text-attr", "ReqIF.ChapterName"]),
    );
    unnamed.assert_code(1, "`(unnamed)` is display only, not a selector");
    assert_eq!(fx.claims(), before);

    let args = base(&[
        "--type",
        DOORS_TYPE,
        "--text-attr",
        "ReqIF.ChapterName",
        "--id-attr",
        "ReqIF.ForeignID",
        "--write",
    ]);
    let out = import(fx.path(), &args);
    out.assert_code(0, "DOORS by identifier");
    let drafts = drafts_of(&out);
    assert_eq!(drafts.len(), 3, "{}", out.stdout);
    for (draft, (id, human, _text)) in drafts.iter().zip(DOORS_OBJECTS) {
        assert_eq!(draft.identifier, id);
        assert_eq!(
            draft.human, human,
            "an integer id attribute is its decimal text"
        );
        let claim = fx.read(&format!("claims/{}.yml", draft.claim));
        assert_eq!(
            field(&claim, "  source: "),
            Some(format!("  source: \"{ORIGIN}#{id}\"").as_str())
        );
    }
    assert_eq!(
        statements(&fx, &drafts),
        DOORS_OBJECTS
            .iter()
            .map(|(_, _, t)| (*t).to_string())
            .collect::<Vec<_>>()
    );
    let recon = assert_adds_up(&out, "DOORS");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (3, 3, 0, 0)
    );
}

#[test]
fn doors_with_the_wrong_text_attribute_skips_all_three_as_no_text() {
    let fx = Fixture::with_public("req-reqif-doors-notext", DOORS);
    let before = fx.claims();
    let out = import(
        fx.path(),
        &base(&["--type", DOORS_TYPE, "--text-attr", "ReqIF.Text", "--write"]),
    );
    out.assert_code(1, "nothing extracted");
    let skips = skips_of(&out);
    assert_eq!(skips.len(), 3, "{}", out.stderr);
    assert!(
        skips.iter().all(|(_, reason)| reason == "no text"),
        "{skips:?}"
    );
    assert_eq!(fx.claims(), before);
    let recon = assert_adds_up(&out, "DOORS, nothing extracted");
    assert_eq!((recon.total, recon.drafted, recon.skipped), (3, 0, 3));
}

// --- (d) RMF -----------------------------------------------------------------------------------------------------

#[test]
fn rmf_has_one_type_two_specifications_and_the_name_attribute_carries_the_text() {
    let fx = Fixture::with_public("req-reqif-rmf", RMF);
    let listed = import(fx.path(), &list_args());
    listed.assert_code(0, "--list on the RMF file");
    let types = listed.lines("type ");
    assert_eq!(types.len(), 1, "{}", listed.stdout);
    assert!(
        types[0].starts_with(&format!("type {RMF_TYPE}  ")) && types[0].ends_with("objects 6"),
        "{types:?}"
    );
    let specs = listed.lines("specification ");
    assert_eq!(specs.len(), 2, "{}", listed.stdout);
    assert_eq!(specs[0], "specification (unnamed)  objects 6");
    assert_eq!(specs[1], "specification (unnamed)  objects 0");
    let indented: Vec<&str> = listed
        .stdout
        .lines()
        .filter(|l| l.starts_with(' '))
        .map(str::trim)
        .collect();
    assert!(
        indented.contains(&"ReqIF.Name  XHTML  6 with a value"),
        "{indented:?}"
    );

    let out = import(
        fx.path(),
        &base(&["--type", RMF_TYPE, "--text-attr", "ReqIF.Name", "--write"]),
    );
    out.assert_code(0, "RMF by type name");
    let drafts = drafts_of(&out);
    assert_eq!(drafts.len(), 6, "{}", out.stdout);
    assert_eq!(
        statements(&fx, &drafts),
        RMF_TEXTS
            .iter()
            .map(|t| (*t).to_string())
            .collect::<Vec<_>>(),
        "the `<p>` wrapper is taken, in file order"
    );
    let recon = assert_adds_up(&out, "RMF");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (6, 6, 0, 0)
    );

    let fx = Fixture::with_public("req-reqif-rmf-text", RMF);
    let before = fx.claims();
    let out = import(
        fx.path(),
        &base(&["--type", RMF_TYPE, "--text-attr", "ReqIF.Text", "--write"]),
    );
    out.assert_code(1, "ReqIF.Text is not defined on RMF's type");
    assert!(
        out.stderr.contains("ReqIF.Text") && out.stderr.contains("none of the selected"),
        "{}",
        out.stderr
    );
    assert_eq!(fx.claims(), before);
}

// --- (e) dry run, ids ------------------------------------------------------------------------------------------

#[test]
fn a_dry_run_writes_nothing_and_a_write_creates_exactly_the_reported_files() {
    let fx = Fixture::with_public("req-reqif-dry", RMF);
    let before = fx.claims();
    let args = base(&["--type", RMF_TYPE, "--text-attr", "ReqIF.Name"]);
    let dry = import(fx.path(), &args);
    dry.assert_code(0, "dry run");
    assert_eq!(fx.claims(), before, "a dry run creates no file");
    assert_eq!(drafts_of(&dry).len(), 6);
    assert!(dry.stdout.contains("dry run"), "{}", dry.stdout);

    let written = import(fx.path(), &plus(args, &["--write"]));
    written.assert_code(0, "--write");
    let reported: Vec<String> = drafts_of(&written)
        .iter()
        .map(|d| format!("{}.yml", d.claim))
        .collect();
    // `req init` made REQ-0001; the drafts are the next six, in order, and nothing else is there.
    let expected: Vec<String> = (1..=7).map(|n| format!("REQ-{n:04}.yml")).collect();
    assert_eq!(claim_numbers(&fx), expected);
    assert_eq!(
        reported,
        (2..=7)
            .map(|n| format!("REQ-{n:04}.yml"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_claim_id_is_the_next_free_number_never_an_identifier_or_a_human_id() {
    // DOORS human ids are 1, 2, 3 and `req init` owns REQ-0001: a claim id taken from them would
    // collide or reuse a number. A gap in the corpus is not reused either (one past the highest).
    let fx = Fixture::with_public("req-reqif-ids", DOORS);
    fx.write(
        "claims/REQ-0007.yml",
        &fx.read("claims/REQ-0001.yml")
            .replace("REQ-0001", "REQ-0007"),
    );
    let out = import(
        fx.path(),
        &base(&[
            "--type",
            DOORS_TYPE,
            "--text-attr",
            "ReqIF.ChapterName",
            "--id-attr",
            "ReqIF.ForeignID",
            "--write",
        ]),
    );
    out.assert_code(0, "DOORS into a corpus with REQ-0007");
    let claims: Vec<String> = drafts_of(&out).into_iter().map(|d| d.claim).collect();
    assert_eq!(claims, vec!["REQ-0008", "REQ-0009", "REQ-0010"]);
    for claim in &claims {
        let text = fx.read(&format!("claims/{claim}.yml"));
        assert_eq!(
            field(&text, "claim_id: "),
            Some(format!("claim_id: {claim}").as_str())
        );
    }
}

fn plus(mut args: Vec<String>, more: &[&str]) -> Vec<String> {
    args.extend(strings(more));
    args
}

// --- (f) provenance --------------------------------------------------------------------------------------------------

#[test]
fn a_second_write_over_the_same_origin_refuses_and_another_origin_is_allowed() {
    let fx = Fixture::with_public("req-reqif-provenance", POLARION);
    let args = base(&[
        "--type",
        "System Requirement",
        "--text-attr",
        "ReqIF.Text",
        "--write",
    ]);
    import(fx.path(), &args).assert_code(0, "the first write");
    let after_first = fx.claims();
    assert_eq!(after_first.len(), 2);

    let again = import(fx.path(), &args);
    again.assert_code(1, "the same origin again");
    assert!(
        again.stderr.contains("REQ-0002") && again.stderr.contains(POLARION_REQ_ID),
        "the refusal names the claim and the object:\n{}",
        again.stderr
    );
    assert_eq!(fx.claims(), after_first, "nothing was overwritten or added");

    let mut other = args.clone();
    let at = other
        .iter()
        .position(|a| a == ORIGIN)
        .expect("--origin value");
    other[at] = "https://example.test/export/another.reqif".to_string();
    import(fx.path(), &other).assert_code(0, "another origin is another provenance");
    assert_eq!(fx.claims().len(), 3);
}

// --- (g) reconciliation ---------------------------------------------------------------------------------------------

#[test]
fn the_last_line_adds_up_on_the_public_files_and_on_synthetic_ones() {
    // Polarion: the plain-string heading has no `ReqIF.ForeignID` value, the requirement does.
    let fx = Fixture::with_public("req-reqif-recon-polarion", POLARION);
    let out = import(
        fx.path(),
        &base(&[
            "--type",
            "Heading",
            "--type",
            "System Requirement",
            "--text-attr",
            "ReqIF.ForeignID",
        ]),
    );
    out.assert_code(0, "Polarion, ForeignID as text");
    let recon = assert_adds_up(&out, "Polarion");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (2, 1, 1, 0)
    );
    assert_eq!(
        skips_of(&out),
        vec![(POLARION_HEADING_ID.to_string(), "no text".to_string())]
    );

    // DOORS: one integer `--where` clause leaves two objects not selected, counted under the
    // type's identifier because the type has no name.
    let fx = Fixture::with_public("req-reqif-recon-doors", DOORS);
    let out = import(
        fx.path(),
        &base(&[
            "--where",
            "ReqIF.ForeignID=2",
            "--text-attr",
            "ReqIF.ChapterName",
        ]),
    );
    out.assert_code(0, "DOORS, ForeignID=2");
    let recon = assert_adds_up(&out, "DOORS");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (3, 1, 0, 2)
    );
    assert_eq!(recon.by_type, Some(format!("{DOORS_TYPE} 2")));
    assert_eq!(drafts_of(&out)[0].identifier, DOORS_OBJECTS[1].0);

    // RMF: every object carries E1 = one.
    let fx = Fixture::with_public("req-reqif-recon-rmf", RMF);
    let out = import(
        fx.path(),
        &base(&["--where", "E1=one", "--text-attr", "ReqIF.Name"]),
    );
    out.assert_code(0, "RMF, E1=one");
    let recon = assert_adds_up(&out, "RMF");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (6, 6, 0, 0)
    );
}

#[test]
fn n_counts_every_object_including_one_under_no_hierarchy_node_and_not_selected_is_by_type() {
    // SYNTHETIC: four objects, one of them (`orphan`) referenced by no hierarchy node.
    let doc = Doc {
        specs: specification("Spec", &node("top", &node("child", ""))),
        ..Doc::new()
    }
    .object(req_xhtml("top", &div("The top shall hold.")))
    .object(req_xhtml("child", &div("The child shall hold.")))
    .object(req_xhtml("orphan", &div("The orphan shall hold.")))
    .object(obj(
        "note-1",
        "T-NOTE",
        &[text_xhtml("AD-NTEXT", &div("A note."))],
    ));
    let fx = synthetic_fixture("req-reqif-recon-synth", &doc);

    let listed = import(fx.path(), &list_args());
    listed.assert_code(0, "--list");
    assert_eq!(
        listed.line("specification "),
        "specification Spec  objects 2",
        "a specification counts the objects its hierarchy reaches"
    );

    let out = import(fx.path(), &base(&REQ_ARGS));
    out.assert_code(0, "synthetic");
    let recon = assert_adds_up(&out, "synthetic");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (4, 3, 0, 1)
    );
    assert_eq!(recon.by_type.as_deref(), Some("Note 1"));
    let hierarchy = out.line("hierarchy");
    assert!(
        hierarchy.contains("1 of 3") && hierarchy.contains("Spec"),
        "only `child` sits under a parent: {hierarchy}"
    );
}

// --- (h) no defaults ---------------------------------------------------------------------------------------------------

#[test]
fn there_is_no_default_selection_and_no_default_text_attribute() {
    let fx = Fixture::with_public("req-reqif-nodefault", POLARION);
    let before = fx.claims();
    let neither = import(fx.path(), &base(&["--text-attr", "ReqIF.Text"]));
    neither.assert_code(2, "neither --type nor --where");
    assert!(
        neither.stderr.contains("--type") && neither.stderr.contains("--where"),
        "{}",
        neither.stderr
    );
    let no_text = import(fx.path(), &base(&["--type", "System Requirement"]));
    no_text.assert_code(2, "no --text-attr");
    assert!(no_text.stderr.contains("--text-attr"), "{}", no_text.stderr);

    let nothing = import(
        fx.path(),
        &base(&[
            "--type",
            "Not A Type",
            "--text-attr",
            "ReqIF.Text",
            "--write",
        ]),
    );
    nothing.assert_code(1, "a --type matching nothing");
    assert!(
        nothing.stderr.contains("Not A Type")
            && nothing.stderr.contains("Heading")
            && nothing.stderr.contains("System Requirement"),
        "the message lists the types the file has:\n{}",
        nothing.stderr
    );
    assert_eq!(fx.claims(), before);
}

#[test]
fn a_malformed_invocation_is_a_usage_error() {
    let fx = Fixture::with_public("req-reqif-usage", POLARION);
    let good = base(&["--type", "System Requirement", "--text-attr", "ReqIF.Text"]);
    for flag in ["--from", "--origin", "--date"] {
        let out = import(fx.path(), &without(good.clone(), flag));
        out.assert_code(2, &format!("no {flag}"));
        assert!(out.stderr.contains(flag), "{flag}: {}", out.stderr);
    }
    for (what, args) in [
        (
            "an empty --origin",
            plus(without(good.clone(), "--origin"), &["--origin", ""]),
        ),
        (
            "an --origin that is a flag",
            plus(without(good.clone(), "--origin"), &["--origin", "-x"]),
        ),
        (
            "a --type that is a flag",
            plus(good.clone(), &["--type", "-x"]),
        ),
        (
            "an empty --where NAME",
            plus(good.clone(), &["--where", "=Draft"]),
        ),
        (
            "a --where without =",
            plus(good.clone(), &["--where", "Status"]),
        ),
        (
            "a bad date",
            plus(without(good.clone(), "--date"), &["--date", "2026-13-40"]),
        ),
        ("--list with --write", plus(list_args(), &["--write"])),
        (
            "--list with --type",
            plus(list_args(), &["--type", "Heading"]),
        ),
        (
            "--list with --where",
            plus(list_args(), &["--where", "Status=Draft"]),
        ),
        (
            "--list with --text-attr",
            plus(list_args(), &["--text-attr", "ReqIF.Text"]),
        ),
        (
            "--list with --id-attr",
            plus(list_args(), &["--id-attr", "ReqIF.ForeignID"]),
        ),
        (
            "--list with --origin",
            plus(list_args(), &["--origin", ORIGIN]),
        ),
        ("--list with --date", plus(list_args(), &["--date", DATE])),
        ("an unknown flag", plus(good.clone(), &["--nope"])),
    ] {
        import(fx.path(), &args).assert_code(2, what);
    }
    assert_eq!(fx.claims().len(), 1, "nothing was written");
}

// --- (i) skips are named (synthetic) ------------------------------------------------------------------------------

#[test]
fn every_skip_is_named_counted_and_nothing_is_flattened() {
    // SYNTHETIC: one object per case, all `Requirement`, plus one draftable and one `Note`.
    let wrapped = |inner: &str| div(inner);
    let cases: Vec<(&str, String, &str)> = vec![
        ("m-blank-div", wrapped("   "), "no text"),
        (
            "m-blank-divs",
            format!("{}<xhtml:div/>", div(" ")),
            "no text",
        ),
        (
            "m-twodivs",
            format!("{}{}", div("One."), div("Two.")),
            "multi-paragraph",
        ),
        ("r-br", wrapped("Line<xhtml:br/>two"), "rich markup (br)"),
        (
            "r-span",
            wrapped("a <xhtml:span>b</xhtml:span>"),
            "rich markup (span)",
        ),
        (
            "r-sub",
            wrapped("m<xhtml:sub>2</xhtml:sub>"),
            "rich markup (sub)",
        ),
        (
            "r-sup",
            wrapped("m<xhtml:sup>2</xhtml:sup>"),
            "rich markup (sup)",
        ),
        (
            "r-ul",
            wrapped("<xhtml:ul><xhtml:li>x</xhtml:li></xhtml:ul>"),
            "rich markup (ul)",
        ),
        (
            "r-table",
            wrapped("<xhtml:table><xhtml:tr><xhtml:td>x</xhtml:td></xhtml:tr></xhtml:table>"),
            "rich markup (table)",
        ),
        (
            "r-object",
            wrapped("<xhtml:object data=\"x.png\" type=\"image/png\">alt</xhtml:object>"),
            "rich markup (object)",
        ),
        (
            "r-wrapper",
            "<xhtml:ul><xhtml:li>x</xhtml:li></xhtml:ul>".to_string(),
            "rich markup (ul)",
        ),
    ];
    let mut doc = Doc::new();
    for (id, inner, _) in &cases {
        doc = doc.object(req_xhtml(id, inner));
    }
    // A missing value, a blank plain string, and a plain string with a blank line.
    doc = doc
        .object(obj(
            "m-missing",
            "T-REQ",
            &[text_string("AD-ID", "no text value here")],
        ))
        .object(obj(
            "m-blank-string",
            "T-REQ",
            &[text_string("AD-TEXT", "   ")],
        ))
        .object(obj(
            "m-plain-blank-line",
            "T-REQ",
            &[text_string("AD-TEXT", "One.&#10;&#10;Two.")],
        ))
        .object(req_xhtml("ok-1", &div("The one shall hold.")))
        .object(obj(
            "note-1",
            "T-NOTE",
            &[text_xhtml("AD-NTEXT", &div("A note."))],
        ));
    let fx = synthetic_fixture("req-reqif-skips", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--write"]));
    out.assert_code(0, "synthetic skips");

    let skips: BTreeMap<String, String> = skips_of(&out).into_iter().collect();
    for (id, _, reason) in &cases {
        assert_eq!(
            skips.get(*id).map(String::as_str),
            Some(*reason),
            "{id}\n{}",
            out.stdout
        );
    }
    assert_eq!(skips.get("m-missing").map(String::as_str), Some("no text"));
    assert_eq!(
        skips.get("m-blank-string").map(String::as_str),
        Some("no text")
    );
    assert_eq!(
        skips.get("m-plain-blank-line").map(String::as_str),
        Some("multi-paragraph")
    );

    let drafts = drafts_of(&out);
    assert_eq!(
        drafts.len(),
        1,
        "only the plain one is drafted:\n{}",
        out.stdout
    );
    assert_eq!(drafts[0].identifier, "ok-1");
    assert_eq!(
        statements(&fx, &drafts),
        vec!["The one shall hold.".to_string()]
    );
    let recon = assert_adds_up(&out, "skips");
    assert_eq!(recon.total, cases.len() + 3 + 1 + 1);
    assert_eq!(recon.skipped, cases.len() + 3);
    assert_eq!(recon.not_selected, 1);

    // `m<sub>2</sub>` is never flattened to `m2`, nor `a <span>b</span>` to `a b`, nor `Line<br/>two`.
    for (name, text) in fx.claims() {
        let statement = field(&text, "statement: ").unwrap_or_default();
        for flattened in [
            "m2",
            "a b",
            "Linetwo",
            "Line two",
            "One. Two.",
            "One.Two.",
            "OneTwo",
        ] {
            assert!(
                !statement.contains(flattened),
                "{name} carries a flattened {flattened:?}:\n{text}"
            );
        }
    }
}

// --- (j) joining ----------------------------------------------------------------------------------------------------------

#[test]
fn a_plain_paragraph_is_joined_with_single_spaces_and_entities_are_decoded() {
    // SYNTHETIC.
    let doc = Doc::new()
        .object(obj(
            "plain",
            "T-REQ",
            &[text_string("AD-TEXT", "  Line one.&#10;   line  two.   ")],
        ))
        .object(req_xhtml(
            "xhtml-div",
            "<xhtml:div>\n   The  tool\n   shall &amp; will.  </xhtml:div>",
        ))
        .object(req_xhtml(
            "xhtml-p",
            "<xhtml:p>  A  paragraph\twith &lt;angle&gt; marks. </xhtml:p>",
        ));
    let fx = synthetic_fixture("req-reqif-join", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--write"]));
    out.assert_code(0, "joining");
    let drafts = drafts_of(&out);
    assert_eq!(
        statements(&fx, &drafts),
        vec![
            "Line one. line two.".to_string(),
            "The tool shall & will.".to_string(),
            "A paragraph with <angle> marks.".to_string(),
        ]
    );
}

// --- (k) where --------------------------------------------------------------------------------------------------------

fn where_doc() -> Doc {
    // SYNTHETIC: five requirements and a note.
    Doc::new()
        .object(obj(
            "w1",
            "T-REQ",
            &[
                text_xhtml("AD-TEXT", &div("First.")),
                text_string("AD-ID", "ALPHA"),
                int_value("AD-NUM", "5"),
                enum_value("AD-STATUS", &["EV-DRAFT"]),
            ],
        ))
        .object(obj(
            "w2",
            "T-REQ",
            &[
                text_xhtml("AD-TEXT", &div("Second.")),
                text_string("AD-ID", "ALPHA"),
                int_value("AD-NUM", "6"),
                enum_value("AD-STATUS", &["EV-APPROVED"]),
            ],
        ))
        .object(obj(
            "w3",
            "T-REQ",
            &[
                text_xhtml("AD-TEXT", &div("Third.")),
                text_string("AD-ID", "BETA"),
                int_value("AD-NUM", "5"),
                enum_value("AD-STATUS", &["EV-DRAFT", "EV-APPROVED"]),
            ],
        ))
        .object(obj(
            "w4",
            "T-REQ",
            &[
                text_xhtml("AD-TEXT", &div("Fourth.")),
                text_string("AD-ID", "x=y"),
            ],
        ))
        .object(obj(
            "w5-note",
            "T-NOTE",
            &[text_xhtml("AD-NTEXT", &div("A note."))],
        ))
}

fn selected(fx: &Fixture, more: &[&str]) -> Vec<String> {
    let out = import(fx.path(), &plus(base(&["--text-attr", "ReqIF.Text"]), more));
    out.assert_code(0, &format!("{more:?}"));
    drafts_of(&out).into_iter().map(|d| d.identifier).collect()
}

#[test]
fn where_clauses_are_anded_split_at_the_first_equals_and_enumerations_match_either_value() {
    let fx = synthetic_fixture("req-reqif-where", &where_doc());
    assert_eq!(selected(&fx, &["--where", "ID=ALPHA"]), ["w1", "w2"]);
    assert_eq!(
        selected(&fx, &["--where", "ID=ALPHA", "--where", "Status=Draft"]),
        ["w1"],
        "two clauses narrow"
    );
    assert_eq!(
        selected(&fx, &["--type", "Requirement", "--where", "ID=BETA"]),
        ["w3"],
        "a --type and a --where both hold"
    );
    assert_eq!(
        selected(&fx, &["--where", "Num=5"]),
        ["w1", "w3"],
        "an integer compares by its decimal text"
    );
    assert_eq!(
        selected(&fx, &["--where", "Status=Approved"]),
        ["w2", "w3"],
        "an enumeration object with two values matches either"
    );
    assert_eq!(
        selected(
            &fx,
            &["--where", "Status=Draft", "--where", "Status=Approved"]
        ),
        ["w3"],
        "all clauses must hold"
    );
    assert_eq!(
        selected(&fx, &["--where", "ID=x=y"]),
        ["w4"],
        "split at the first `=`"
    );

    let none = import(
        fx.path(),
        &base(&[
            "--type",
            "Note",
            "--where",
            "ID=ALPHA",
            "--text-attr",
            "ReqIF.Text",
        ]),
    );
    none.assert_code(1, "a Note has no ID attribute");
}

#[test]
fn a_name_matching_two_definitions_on_one_type_is_unverifiable() {
    // SYNTHETIC: `Requirement` gets a second `ID` definition.
    let doc = Doc {
        extra_defs: "<ATTRIBUTE-DEFINITION-STRING IDENTIFIER=\"AD-ID2\" LONG-NAME=\"ID\"><TYPE><DATATYPE-DEFINITION-STRING-REF>DT-S</DATATYPE-DEFINITION-STRING-REF></TYPE></ATTRIBUTE-DEFINITION-STRING>".to_string(),
        ..Doc::new()
    }
    .object(obj(
        "a",
        "T-REQ",
        &[text_xhtml("AD-TEXT", &div("First.")), text_string("AD-ID", "ALPHA")],
    ));
    let fx = synthetic_fixture("req-reqif-ambiguous", &doc);
    let before = fx.claims();
    for (what, more) in [
        (
            "--where",
            vec![
                "--type",
                "Requirement",
                "--where",
                "ID=ALPHA",
                "--text-attr",
                "ReqIF.Text",
            ],
        ),
        (
            "--text-attr",
            vec!["--type", "Requirement", "--text-attr", "ID"],
        ),
        (
            "--id-attr",
            vec![
                "--type",
                "Requirement",
                "--text-attr",
                "ReqIF.Text",
                "--id-attr",
                "ID",
            ],
        ),
    ] {
        let out = import(fx.path(), &plus(base(&more), &["--write"]));
        out.assert_code(3, &format!("{what} names two definitions"));
        assert!(out.stderr.starts_with("[UNVERIFIABLE]"), "{}", out.stderr);
        assert!(
            out.stderr.contains("ID") && out.stderr.contains("two"),
            "{}",
            out.stderr
        );
    }
    assert_eq!(fx.claims(), before);
}

// --- (l) not carried ------------------------------------------------------------------------------------------------------

#[test]
fn no_attribute_enumeration_hierarchy_or_relation_reaches_a_claim_and_the_report_says_so() {
    let fx = Fixture::with_public("req-reqif-notcarried", POLARION);
    let out = import(
        fx.path(),
        &base(&[
            "--type",
            "System Requirement",
            "--text-attr",
            "ReqIF.Text",
            "--id-attr",
            "ReqIF.ForeignID",
            "--write",
        ]),
    );
    out.assert_code(0, "Polarion");
    let carried = out.line("not carried");
    for part in [
        "Status 1",
        "ReqIF.ForeignCreatedBy 1",
        "ReqIF.ChapterName 1",
        "ReqIF.ForeignCreatedOn 1",
    ] {
        assert!(
            carried.contains(part),
            "the not carried line names `{part}`: {carried}"
        );
    }
    assert!(
        !carried.contains("ReqIF.Text"),
        "the text attribute is carried: {carried}"
    );
    assert!(
        out.line("relations").contains("0 in the file"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("IDENTIFIER") && out.stdout.contains("provenance"),
        "the report says the IDENTIFIERs are provenance, not the claim_id:\n{}",
        out.stdout
    );

    for (name, text) in fx.claims() {
        if name == "REQ-0001.yml" {
            continue;
        }
        for value in [
            "redacted@mail.com",
            "SW: Lorem Ipsum",
            "2023-03-15",
            "Status",
            "Draft",
            "Approved",
        ] {
            assert!(!text.contains(value), "{name} carries {value:?}:\n{text}");
        }
        assert_eq!(field(&text, "relations: "), Some("relations: []"));
        assert_eq!(field(&text, "applicability: "), Some("applicability: []"));
    }
}

#[test]
fn relations_are_counted_and_named_and_no_claim_has_one() {
    // SYNTHETIC: four requirements and two relations touching three of them.
    let mut doc = Doc::new();
    for id in ["a", "b", "c", "d"] {
        doc = doc.object(req_xhtml(id, &div(&format!("The {id} shall hold."))));
    }
    doc.relations = format!(
        "{}{}",
        relation("R1", "RT-1", "a", "b"),
        relation("R2", "RT-2", "b", "c")
    );
    let fx = synthetic_fixture("req-reqif-relations", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--write"]));
    out.assert_code(0, "relations");
    let line = out.line("relations");
    assert!(
        line.contains("2 in the file")
            && line.contains("3 of 4")
            && line.contains("Refines")
            && line.contains("Verifies"),
        "{line}"
    );
    for (name, text) in fx.claims() {
        assert_eq!(field(&text, "relations: "), Some("relations: []"), "{name}");
    }
}

// --- (m) UNVERIFIABLE --------------------------------------------------------------------------------------------------------

#[test]
fn a_file_that_cannot_be_read_honestly_is_unverifiable_names_which_and_writes_nothing() {
    let valid = Doc::new()
        .object(req_xhtml("a", &div("The a shall hold.")))
        .render();
    let ok_object = |text: &str| req_xhtml("ok", &div(text));
    let cases: Vec<(&str, String, Vec<&str>)> = vec![
        ("not XML", "this is not xml at all".to_string(), vec!["XML"]),
        (
            "a DOCTYPE",
            valid.replacen("<REQ-IF ", "<!DOCTYPE REQ-IF>\n<REQ-IF ", 1),
            vec!["DOCTYPE"],
        ),
        (
            "an entity declaration",
            valid
                .replacen(
                    "<REQ-IF ",
                    "<!DOCTYPE REQ-IF [<!ENTITY marker \"EXPANDED-MARKER-7f3a\">]>\n<REQ-IF ",
                    1,
                )
                .replace("The a shall hold.", "The &marker; shall hold."),
            vec!["entity"],
        ),
        (
            // The refusal comes before the parse: a DOCTYPE is named even when the file would not
            // have parsed anyway, so nothing was handed to the parser to find out.
            "a DOCTYPE ahead of a file that is not well-formed",
            format!(
                "<?xml version=\"1.0\"?>\n<!DOCTYPE REQ-IF>\n<REQ-IF xmlns=\"{NS}\"><unclosed>"
            ),
            vec!["DOCTYPE"],
        ),
        (
            "an entity declaration ahead of a file that is not well-formed",
            format!(
                "<?xml version=\"1.0\"?>\n<!DOCTYPE REQ-IF [<!ENTITY x \"y\">]>\n<REQ-IF xmlns=\"{NS}\"><unclosed>"
            ),
            vec!["entity"],
        ),
        (
            "a root that is not REQ-IF",
            format!("<?xml version=\"1.0\"?><OTHER xmlns=\"{NS}\"/>"),
            vec!["REQ-IF"],
        ),
        (
            "the wrong namespace",
            valid.replace(NS, "http://www.omg.org/spec/ReqIF/20101201/reqif.xsd"),
            vec!["20110401"],
        ),
        (
            "an object with no IDENTIFIER",
            valid.replace("<SPEC-OBJECT IDENTIFIER=\"a\"", "<SPEC-OBJECT "),
            vec!["IDENTIFIER"],
        ),
        (
            "two objects with the same IDENTIFIER",
            Doc::new()
                .object(ok_object("The first shall hold."))
                .object(ok_object("The second shall hold."))
                .render(),
            vec!["ok", "twice"],
        ),
        (
            "a dangling definition reference",
            valid.replace(
                "<ATTRIBUTE-DEFINITION-XHTML-REF>AD-TEXT<",
                "<ATTRIBUTE-DEFINITION-XHTML-REF>AD-NOPE<",
            ),
            vec!["AD-NOPE"],
        ),
        (
            "a dangling enumeration value reference",
            Doc::new()
                .object(obj(
                    "a",
                    "T-REQ",
                    &[
                        text_xhtml("AD-TEXT", &div("The a shall hold.")),
                        enum_value("AD-STATUS", &["EV-NOPE"]),
                    ],
                ))
                .render(),
            vec!["EV-NOPE"],
        ),
        (
            "an object whose type does not resolve",
            Doc::new().object(obj("a", "T-NOPE", &[])).render(),
            vec!["T-NOPE"],
        ),
    ];
    for (what, text, names) in cases {
        let fx = Fixture::with_reqif("req-reqif-unverifiable", &text);
        let before = fx.claims();
        let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--write"]));
        out.assert_code(3, what);
        assert!(
            out.stderr.starts_with("[UNVERIFIABLE]"),
            "{what}: tri-state, not a crash:\n{}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("internal error") && !out.stderr.contains("Traceback"),
            "{what}: named, not an internal error:\n{}",
            out.stderr
        );
        for name in names {
            assert!(
                out.stderr.contains(name),
                "{what}: names `{name}`:\n{}",
                out.stderr
            );
        }
        assert!(
            !out.stdout.contains("EXPANDED-MARKER-7f3a")
                && !out.stderr.contains("EXPANDED-MARKER-7f3a"),
            "{what}: an entity is never expanded"
        );
        assert_eq!(fx.claims(), before, "{what}: nothing written");
        // `--list` refuses the same files in the same way.
        let listed = import(fx.path(), &list_args());
        listed.assert_code(3, &format!("{what}: --list"));
    }
}

#[test]
fn a_reqifz_path_is_unverifiable_and_says_to_unzip_it() {
    let fx = Fixture::with_reqif("req-reqif-reqifz", &Doc::new().render());
    fx.write("module.reqifz", "PK not read");
    let before = fx.claims();
    let mut args = base(&REQ_ARGS);
    args[1] = "module.reqifz".to_string();
    let out = import(fx.path(), &plus(args, &["--write"]));
    out.assert_code(3, ".reqifz");
    assert!(
        out.stderr.contains(".reqifz") && out.stderr.contains("unzip"),
        "{}",
        out.stderr
    );
    assert_eq!(fx.claims(), before);
}

#[test]
fn a_repeated_identifier_on_non_objects_is_accepted() {
    // The Polarion file repeats one IDENTIFIER on two SPEC-HIERARCHY nodes (never on objects).
    let bytes = fs::read_to_string(public_path(POLARION)).expect("reads the fixture");
    assert_eq!(
        bytes
            .matches("IDENTIFIER=\"rmf-0c4d996f-31e9-41d5-bbf0-73c13fc68f3c\"")
            .count(),
        2,
        "the fixture still has the repeated hierarchy IDENTIFIER"
    );
    let fx = Fixture::with_public("req-reqif-repeat", POLARION);
    import(fx.path(), &list_args()).assert_code(0, "--list");
    import(
        fx.path(),
        &base(&["--type", "System Requirement", "--text-attr", "ReqIF.Text"]),
    )
    .assert_code(0, "a drafting run");
}

#[test]
fn a_corpus_the_loader_rejects_is_unverifiable_and_nothing_is_drafted() {
    let current = req::canon::claim_schema();
    let valid = |schema_line: &str, extra: &str| {
        format!(
            "{schema_line}\nclaim_id: REQ-0009\nstatement: An existing claim.\napplicability: []\nrelations: []\n\
             rationale:\n  source: x\n  date: 2026-10-07\n  rejected_alternatives: []\n  assumptions: []\n{extra}"
        )
    };
    for (name, text) in [
        (
            "removed-key",
            valid(&format!("schema: {current}"), "modality: SHALL\n"),
        ),
        ("no-schema", valid("# no schema here", "")),
        (
            "bad-pin",
            valid(&format!("schema: {current}"), "pins:\n  value: \"0.30\"\n"),
        ),
    ] {
        let fx = Fixture::with_public("req-reqif-reject", POLARION);
        fx.write("claims/REQ-0009.yml", &text);
        let before = fx.claims();
        let out = import(
            fx.path(),
            &base(&[
                "--type",
                "System Requirement",
                "--text-attr",
                "ReqIF.Text",
                "--write",
            ]),
        );
        out.assert_code(3, &format!("{name}: a corpus the loader rejects"));
        assert!(out.stderr.contains("req canon"), "{name}: {}", out.stderr);
        assert_eq!(fx.claims(), before, "{name}: nothing is drafted into it");
    }
}

#[cfg(unix)]
#[test]
fn a_req_reporting_another_claim_schema_is_unverifiable_and_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::with_public("req-reqif-schema", POLARION);
    let bin = fx.path().join("fake-bin");
    fs::create_dir_all(&bin).expect("fake bin dir");
    let fake = bin.join("req");
    fs::write(
        &fake,
        "#!/bin/sh\nprintf 'req 0.1.0\\n  rules_v            6\\n  claim_schema       req.claim/4\\n'\n",
    )
    .expect("fake req");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("chmod");
    let before = fx.claims();
    let out = import_in(
        fx.path(),
        &base(&[
            "--type",
            "System Requirement",
            "--text-attr",
            "ReqIF.Text",
            "--write",
        ]),
        &Env {
            path_first: Some(&bin),
            ..Env::default()
        },
    );
    out.assert_code(3, "a req that judges another claim schema");
    assert!(
        out.stderr.contains("req.claim/4") && out.stderr.contains("req.claim/2"),
        "{}",
        out.stderr
    );
    assert_eq!(fx.claims(), before);
}

/// ADR 0051 decision 6, for the importer: it drafts `req.claim/2` and accepts a `req` that reports
/// `req.claim/2` or `req.claim/3`; any other value is refused with the message above.
#[cfg(unix)]
#[test]
fn a_req_reporting_claim_schema_2_or_3_is_accepted_and_4_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    for (reported, accepted) in [
        ("req.claim/2", true),
        ("req.claim/3", true),
        ("req.claim/4", false),
        ("req.claim/1", false),
    ] {
        let fx = Fixture::with_public("req-reqif-schema-accept", POLARION);
        let bin = fx.path().join("fake-bin");
        fs::create_dir_all(&bin).expect("fake bin dir");
        let fake = bin.join("req");
        fs::write(
            &fake,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'req 0.1.0\\n  rules_v            9\\n  claim_schema       {reported}\\n'\nelse\n  exec \"{}\" \"$@\"\nfi\n",
                req_bin().display()
            ),
        )
        .expect("fake req");
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("chmod");
        let before = fx.claims();
        let out = import_in(
            fx.path(),
            &base(&[
                "--type",
                "System Requirement",
                "--text-attr",
                "ReqIF.Text",
                "--write",
            ]),
            &Env {
                path_first: Some(&bin),
                ..Env::default()
            },
        );
        if accepted {
            out.assert_code(0, &format!("a req reporting {reported}"));
            let drafted: Vec<String> = fx
                .claims()
                .into_iter()
                .filter(|(name, _)| !before.contains_key(name))
                .map(|(_, text)| text)
                .collect();
            assert!(!drafted.is_empty(), "{reported}: something was drafted");
            for text in drafted {
                assert!(
                    text.lines().any(|l| l == "schema: req.claim/2"),
                    "{reported}: the draft stays req.claim/2:\n{text}"
                );
            }
        } else {
            out.assert_code(3, &format!("a req reporting {reported}"));
            assert!(
                out.stderr.contains(reported) && out.stderr.contains("req.claim/2"),
                "{reported}: {}",
                out.stderr
            );
            assert_eq!(fx.claims(), before, "{reported}: nothing is drafted");
        }
    }
}

// --- (n) dates ----------------------------------------------------------------------------------------------------------------------

#[test]
fn the_date_is_authored_and_never_the_clock_a_commit_or_the_export_time() {
    let fx = Fixture::with_public("req-reqif-date", POLARION);
    let args = base(&["--type", "System Requirement", "--text-attr", "ReqIF.Text"]);
    let missing = import(fx.path(), &without(args.clone(), "--date"));
    missing.assert_code(2, "no --date");
    assert!(missing.stderr.contains("--date"), "{}", missing.stderr);
    for bad in [
        "2026-13-40",
        "2026-1-2",
        "2026-02-30",
        "yesterday",
        "2026-11-05T00:00:00Z",
    ] {
        let mut with_bad = without(args.clone(), "--date");
        with_bad.extend(strings(&["--date", bad]));
        let out = import(fx.path(), &with_bad);
        out.assert_code(2, &format!("--date {bad}"));
        assert!(out.stderr.contains("--date"), "{}", out.stderr);
    }
    assert_eq!(fx.claims().len(), 1, "nothing was written");

    // The file's CREATION-TIME (2021-07-01…) differs from --date and is not used for it.
    let out = import(fx.path(), &plus(args, &["--write"]));
    out.assert_code(0, "with --date");
    let drafts = drafts_of(&out);
    let claim = fx.read(&format!("claims/{}.yml", drafts[0].claim));
    assert!(claim.contains(&format!("  date: {DATE}\n")), "{claim}");
    assert!(!claim.contains("2021-07-01"), "{claim}");
    assert!(
        out.stdout.contains(&format!("date: {DATE} (--date)")),
        "{}",
        out.stdout
    );

    // The source never asks for the time or for a commit.
    let source = fs::read_to_string(script()).expect("reads the script");
    for forbidden in [
        "date.today",
        ".now(",
        "utcnow",
        "time.time",
        "strftime",
        "\"git\"",
        "'git'",
        "git log",
        "git -C",
    ] {
        assert!(
            !source.contains(forbidden),
            "tools/reqif_import.py must not contain `{forbidden}`"
        );
    }
}

// --- (o) nothing derived ----------------------------------------------------------------------------------------------------

#[test]
fn a_draft_carries_nothing_derived_and_the_strict_loader_accepts_it() {
    let fx = Fixture::with_public("req-reqif-derived", RMF);
    let out = import(
        fx.path(),
        &base(&["--type", RMF_TYPE, "--text-attr", "ReqIF.Name", "--write"]),
    );
    out.assert_code(0, "RMF");
    // The importers draft `req.claim/2` and do not follow the build's format: a draft carries no area, and
    // a `req.claim/3` build loads `req.claim/2` as it is (ADR 0051 decision 6, task A-1 choice 7).
    let schema = "req.claim/2";
    for draft in drafts_of(&out) {
        let text = fx.read(&format!("claims/{}.yml", draft.claim));
        assert_eq!(
            field(&text, "schema: "),
            Some(format!("schema: {schema}").as_str()),
            "{text}"
        );
        for key in [
            "modality:",
            "subject:",
            "attribute:",
            "operator:",
            "value:",
            "unit:",
            "conditions:",
            "exception:",
            "temporal:",
            "quantifier:",
            "unstructured:",
            "canonical:",
            "address:",
            "pins:",
        ] {
            assert_eq!(
                field(&text, key),
                None,
                "{} carries nothing derived (`{key}`):\n{text}",
                draft.claim
            );
        }
        assert_eq!(field(&text, "applicability: "), Some("applicability: []"));
        assert_eq!(field(&text, "relations: "), Some("relations: []"));
        assert!(
            text.contains("rejected_alternatives: []") && text.contains("assumptions: []"),
            "{text}"
        );
    }
    let canon = req(fx.path(), &["canon"]);
    canon.assert_code(0, "the strict loader accepts the drafts");
    assert!(canon.stdout.contains("claims: 7"), "{}", canon.stdout);
}

// --- (p) --self-check and --list stand alone ---------------------------------------------------------------------------------

#[test]
fn the_self_check_stands_alone_and_refuses_company() {
    let bare = Fixture::new("req-reqif-selfcheck");
    let out = Command::new(python3())
        .arg(script())
        .arg("--self-check")
        .current_dir(bare.path())
        .env("PATH", "")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/reqif_import.py --self-check");
    let out = outcome(&out);
    out.assert_code(0, "--self-check with nothing around it");
    let cases: usize = out
        .stdout
        .split("reqif_import --self-check: ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("the self-check summary is missing:\n{}", out.stdout));
    assert!(
        cases >= 25,
        "the self-check table has lost rows ({cases}):\n{}",
        out.stdout
    );
    assert!(out.stdout.contains("0 failure(s)"), "{}", out.stdout);

    for extra in [
        vec!["--write"],
        vec!["--from", "module.reqif"],
        vec!["--type", "x"],
        vec!["--list"],
    ] {
        let mut args = vec!["--self-check"];
        args.extend(extra);
        import(bare.path(), &strings(&args))
            .assert_code(2, &format!("--self-check beside {args:?}"));
    }
}

#[test]
fn list_needs_no_corpus_and_no_req() {
    let bare = Fixture::new("req-reqif-list-bare");
    let bytes = fs::read(public_path(RMF)).expect("reads the fixture");
    fs::write(bare.path().join("module.reqif"), bytes).expect("copies the fixture");
    let out = Command::new(python3())
        .arg(script())
        .args(["--from", "module.reqif", "--list"])
        .current_dir(bare.path())
        .env("PATH", "")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/reqif_import.py --list");
    let out = outcome(&out);
    out.assert_code(0, "--list with no corpus and no req on PATH");
    assert!(
        out.stdout.contains("type TC18xx SpecObjectType"),
        "{}",
        out.stdout
    );
    assert!(
        !bare.path().join("claims").exists(),
        "--list never creates a corpus"
    );
}

// --- (q) the corpus root -----------------------------------------------------------------------------------------------------

#[test]
fn the_corpus_root_is_named_or_the_working_directory_never_an_ancestor() {
    let fx = Fixture::with_public("req-reqif-root", POLARION);
    let sub = fx.path().join("sub");
    fs::create_dir_all(&sub).expect("creates the subdirectory");
    let before = fx.claims();
    let args = base(&[
        "--type",
        "System Requirement",
        "--text-attr",
        "ReqIF.Text",
        "--write",
    ]);

    let from_sub = import(&sub, &args);
    from_sub.assert_code(
        3,
        "a subdirectory is not a corpus, and an ancestor is never searched",
    );
    assert!(
        from_sub.stderr.contains("claims/") && from_sub.stderr.contains("sub"),
        "{}",
        from_sub.stderr
    );
    assert_eq!(fx.claims(), before, "nothing was drafted into the ancestor");

    let rooted = import(&sub, &plus(args.clone(), &["--root", ".."]));
    rooted.assert_code(0, "--root ..");
    assert_eq!(fx.claims().len(), before.len() + 1);

    // REQ_ROOT is the next rule; --root beats it.
    let other = Fixture::with_public("req-reqif-root-env", POLARION);
    let env_run = import_in(
        &sub,
        &args,
        &Env {
            req_root: Some(other.path()),
            ..Env::default()
        },
    );
    env_run.assert_code(0, "REQ_ROOT names the corpus");
    assert_eq!(other.claims().len(), 2);
    let mut third = args.clone();
    let at = third
        .iter()
        .position(|a| a == ORIGIN)
        .expect("--origin value");
    third[at] = "https://example.test/third.reqif".to_string();
    let beaten = import_in(
        &sub,
        &plus(
            third,
            &["--root", fx.path().to_str().expect("a UTF-8 path")],
        ),
        &Env {
            req_root: Some(other.path()),
            ..Env::default()
        },
    );
    beaten.assert_code(0, "--root beats REQ_ROOT");
    assert!(
        beaten
            .stdout
            .contains(fx.path().to_str().expect("a UTF-8 path")),
        "the report names the chosen root:\n{}",
        beaten.stdout
    );
}

// --- (r) equal text is not collapsed ---------------------------------------------------------------------------------------------

#[test]
fn equal_statements_are_two_drafts_with_one_address() {
    // SYNTHETIC: two objects with the same text.
    let doc = Doc::new()
        .object(req_xhtml(
            "same-1",
            &div("The system shall log every request."),
        ))
        .object(req_xhtml(
            "same-2",
            &div("The system shall log every request."),
        ));
    let fx = synthetic_fixture("req-reqif-equal", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--write"]));
    out.assert_code(0, "equal text");
    let drafts = drafts_of(&out);
    assert_eq!(drafts.len(), 2, "two drafts, never one: {}", out.stdout);
    let rows = render_rows(fx.path());
    let first = &rows[&drafts[0].claim];
    let second = &rows[&drafts[1].claim];
    assert_eq!(
        first.0, second.0,
        "req shows the same address for the same statement"
    );
    assert_eq!(first.1, second.1);
    req(fx.path(), &["canon"]).assert_code(0, "req canon");
}

// --- the public files are untouched ---------------------------------------------------------------------------------------------

#[test]
fn the_public_fixtures_are_the_pinned_bytes() {
    // The expected values above were read from these bytes; a changed fixture is changed evidence.
    for (file, size) in [(DOORS, 286_685_u64), (POLARION, 11_775), (RMF, 16_080)] {
        let len = fs::metadata(public_path(file)).expect("the fixture").len();
        assert_eq!(len, size, "{file} is not the pinned file");
    }
}

// --- review of #137: findings turned into tests -------------------------------------------------------------------------

#[test]
fn objects_split_over_several_containers_are_all_counted() {
    // SYNTHETIC: the objects sit in two `SPEC-OBJECTS` containers; the report must account for all four.
    let doc = Doc::new()
        .object(req_xhtml("c1-a", &div("The first shall hold.")))
        .object(req_xhtml("c1-b", &div("The second shall hold.")))
        .object("</SPEC-OBJECTS>\n<SPEC-OBJECTS>\n".to_string())
        .object(req_xhtml("c2-a", &div("The third shall hold.")))
        .object(obj(
            "c2-note",
            "T-NOTE",
            &[text_xhtml("AD-NTEXT", &div("A note."))],
        ));
    let fx = synthetic_fixture("req-reqif-containers", &doc);
    let out = import(fx.path(), &base(&REQ_ARGS));
    out.assert_code(0, "two containers");
    let recon = assert_adds_up(&out, "two containers");
    assert_eq!(
        (
            recon.total,
            recon.drafted,
            recon.skipped,
            recon.not_selected
        ),
        (4, 3, 0, 1),
        "{}",
        out.stdout
    );
}

#[test]
fn a_value_element_in_a_foreign_namespace_is_not_read_as_a_reqif_value() {
    // SYNTHETIC: `r2` carries an `ATTRIBUTE-VALUE-STRING` that is not in the ReqIF namespace, with a
    // ReqIF `DEFINITION` inside. Elements are matched by their namespace-qualified name, so it is no value.
    let foreign = "<o:ATTRIBUTE-VALUE-STRING xmlns:o=\"urn:foreign\" THE-VALUE=\"Foreign text.\"><DEFINITION><ATTRIBUTE-DEFINITION-STRING-REF>AD-ID</ATTRIBUTE-DEFINITION-STRING-REF></DEFINITION></o:ATTRIBUTE-VALUE-STRING>".to_string();
    let doc = Doc::new()
        .object(obj("r1", "T-REQ", &[text_string("AD-ID", "Real text.")]))
        .object(obj("r2", "T-REQ", &[foreign]));
    let fx = synthetic_fixture("req-reqif-foreign", &doc);
    let out = import(
        fx.path(),
        &base(&["--type", "Requirement", "--text-attr", "ID"]),
    );
    out.assert_code(0, "foreign namespace");
    let ids: Vec<String> = drafts_of(&out).into_iter().map(|d| d.identifier).collect();
    assert_eq!(ids, ["r1"], "{}", out.stdout);
    assert_eq!(
        skips_of(&out),
        [("r2".to_string(), "no text".to_string())],
        "the foreign element is not a value"
    );
    assert!(
        !out.stdout.contains("Foreign text."),
        "a foreign element's text never reaches a report or a statement"
    );
}

#[test]
fn the_nothing_selected_message_blames_only_the_clause_that_selects_nothing() {
    let fx = synthetic_fixture("req-reqif-where-msg", &where_doc());
    // `Status=Rejected` selects no object; `Num=5` selects some on its own.
    let out = import(
        fx.path(),
        &base(&[
            "--text-attr",
            "ReqIF.Text",
            "--where",
            "Status=Rejected",
            "--where",
            "Num=5",
        ]),
    );
    out.assert_code(1, "nothing selected");
    assert!(
        out.stderr
            .contains("--where Status=Rejected selects no object"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("--where Num=5 selects objects on its own"),
        "a clause that selects objects is not blamed: {}",
        out.stderr
    );
    // Two clauses that each select something but not together: neither is "no object".
    let both = import(
        fx.path(),
        &base(&[
            "--text-attr",
            "ReqIF.Text",
            "--where",
            "ID=BETA",
            "--where",
            "Num=6",
        ]),
    );
    both.assert_code(1, "nothing selected together");
    assert!(
        !both.stderr.contains("selects no object;"),
        "neither clause alone selects nothing: {}",
        both.stderr
    );
}

#[test]
fn the_already_drafted_refusal_still_adds_up() {
    let fx = Fixture::with_public("req-reqif-refusal-recon", POLARION);
    let args = base(&[
        "--type",
        "System Requirement",
        "--text-attr",
        "ReqIF.Text",
        "--write",
    ]);
    import(fx.path(), &args).assert_code(0, "the first write");
    let again = import(fx.path(), &args);
    again.assert_code(1, "the same origin again");
    assert!(
        again
            .stderr
            .contains("objects 2 = drafted 1 + skipped 0 + not selected 1"),
        "the refusal carries the accounting line:\n{}",
        again.stderr
    );
}

// --- (q) --areas: the specification hierarchy as the claim's area (ADR 0051 decision 2, task I-1) ------------------------------
//
// Oracles are ADR 0051, `docs/specs/I-1-importer-areas.md` and hand-written ReqIF: the shared one is
// `tests/fixtures/reqif/areas-sample.reqif` (its header comment draws the hierarchy), the variants are
// synthetic `Doc`s. No expected value is read back from a run of the script.

const AREAS_SAMPLE: &str = "areas-sample.reqif";

/// The arguments every sample run shares: the requirement type, its text, and the human id.
const SAMPLE_ARGS: [&str; 6] = [
    "--type",
    "Requirement",
    "--text-attr",
    "ReqIF.Text",
    "--id-attr",
    "ID",
];

/// The sample's registry with the heading text as the label (a heading has `ReqIF.ChapterName`; the
/// requirement `R-1`, an ancestor too, has none and falls back to its `ID`).
const SAMPLE_REGISTRY: &str = "h-0/h-1 Démarrage 起動\nh-0/h-1/r-1 SW-1\nh-0 Diagnostics\n";

fn sample_run(fx: &Fixture, more: &[&str]) -> Outcome {
    let mut args = base(&SAMPLE_ARGS);
    args.extend(strings(&[
        "--areas",
        "--area-label-attr",
        "ReqIF.ChapterName",
    ]));
    args.extend(strings(more));
    import(fx.path(), &args)
}

/// The claim a run drafted from the object `ident`, by the report's own draft line.
fn claim_of(fx: &Fixture, out: &Outcome, ident: &str) -> String {
    let drafts = drafts_of(out);
    let draft = drafts
        .iter()
        .find(|d| d.identifier == ident)
        .unwrap_or_else(|| panic!("{ident} is drafted:\n{}", out.stdout));
    fx.read(&format!("claims/{}.yml", draft.claim))
}

/// The line after `claim_id:` and the `schema:` line, so a test can say where `area:` sits.
fn schema_of(claim: &str) -> &str {
    field(claim, "schema: ").expect("a schema line")
}

fn area_of(claim: &str) -> Option<&str> {
    field(claim, "area: ")
}

/// The registry lines a report proposes: the indented lines right after its `registry:` line,
/// untrimmed, so a test can say what marks them.
fn proposal_lines(out: &Outcome) -> Vec<&str> {
    out.stdout
        .lines()
        .skip_while(|line| !line.starts_with("registry:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .collect()
}

fn fake_req_reporting(fx: &Fixture, reported: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = fx.path().join("fake-bin");
    fs::create_dir_all(&bin).expect("fake bin dir");
    let fake = bin.join("req");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'req 0.1.0\\n  rules_v            9\\n  claim_schema       {reported}\\n'\nelse\n  exec \"{}\" \"$@\"\nfi\n",
            req_bin().display()
        ),
    )
    .expect("fake req");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

#[test]
fn areas_are_off_by_default_a_run_without_the_flag_changes_no_byte() {
    let fx = Fixture::with_public("req-reqif-areas-off", AREAS_SAMPLE);
    let out = import(fx.path(), &plus(base(&SAMPLE_ARGS), &["--write"]));
    out.assert_code(0, "no --areas, on a file with a hierarchy");
    let drafts = drafts_of(&out);
    assert_eq!(drafts.len(), 6, "{}", out.stdout);

    let r3 = claim_of(&fx, &out, "R-3");
    let claim_id = &drafts.iter().find(|d| d.identifier == "R-3").unwrap().claim;
    let expected = format!(
        "# A draft claim, drafted from the ReqIF file module.reqif (object R-3; ID SW-3). The object's\n\
         # IDENTIFIER is kept in rationale.source. Nothing derived is stored: `req canon` shows the parse\n\
         # this statement derives. A draft is a proposal (ADR 0004): edit the statement, add the\n\
         # applicability and relations it needs, and pin a load-bearing bound under `pins:` if it has one.\n\
         schema: req.claim/2\n\
         claim_id: {claim_id}\n\
         statement: \"The R-3 shall hold.\"\n\
         applicability: []\n\
         rationale:\n\
         \x20 source: \"{ORIGIN}#R-3\"\n\
         \x20 date: {DATE}\n\
         \x20 rejected_alternatives: []\n\
         \x20 assumptions: []\n\
         relations: []\n"
    );
    assert_eq!(r3, expected);
    for (name, text) in fx.claims() {
        assert!(area_of(&text).is_none(), "{name} carries an area:\n{text}");
    }
    for prefix in ["areas:", "no area:", "registry:"] {
        assert!(
            out.lines(prefix).is_empty(),
            "no `{prefix}` line without --areas:\n{}",
            out.stdout
        );
    }
    assert!(
        out.line("hierarchy")
            .contains("the structure is not carried"),
        "{}",
        out.stdout
    );
    assert!(
        !fx.path().join(".req").exists(),
        "no registry is created without --areas"
    );
    assert_adds_up(&out, "areas off");
}

#[test]
fn an_object_under_two_headings_is_filed_under_its_chain_of_ancestors() {
    let fx = Fixture::with_public("req-reqif-areas-two-levels", AREAS_SAMPLE);
    let out = sample_run(&fx, &["--write"]);
    out.assert_code(0, "areas, the sample");
    let r1 = claim_of(&fx, &out, "R-1");
    assert_eq!(schema_of(&r1), "schema: req.claim/3", "{r1}");
    assert_eq!(area_of(&r1), Some("area: h-0/h-1"), "{r1}");
    // `area:` is the line right after `claim_id:`, and the rest of the template is as it was.
    let lines: Vec<&str> = r1.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("claim_id: "))
        .expect("a claim_id line");
    assert_eq!(lines[at + 1], "area: h-0/h-1", "{r1}");
    assert!(lines[at + 2].starts_with("statement: "), "{r1}");
    let r3 = claim_of(&fx, &out, "R-3");
    assert_eq!(area_of(&r3), Some("area: h-0"), "{r3}");
}

#[test]
fn a_drafted_parent_is_filed_under_its_own_ancestors_and_is_a_segment_of_its_children() {
    let fx = Fixture::with_public("req-reqif-areas-parent", AREAS_SAMPLE);
    let out = sample_run(&fx, &["--write"]);
    out.assert_code(0, "areas, a drafted parent");
    let parent = claim_of(&fx, &out, "R-1");
    let child = claim_of(&fx, &out, "R-2");
    assert_eq!(area_of(&parent), Some("area: h-0/h-1"), "{parent}");
    assert_eq!(area_of(&child), Some("area: h-0/h-1/r-1"), "{child}");
}

#[test]
fn no_ancestor_and_several_places_are_filed_nowhere_and_counted_by_reason() {
    let fx = Fixture::with_public("req-reqif-areas-no-area", AREAS_SAMPLE);
    let out = sample_run(&fx, &["--write"]);
    out.assert_code(0, "areas, objects with no area");
    for ident in ["R-4", "R-5", "R-6"] {
        let claim = claim_of(&fx, &out, ident);
        assert_eq!(
            schema_of(&claim),
            "schema: req.claim/2",
            "{ident}:\n{claim}"
        );
        assert!(area_of(&claim).is_none(), "{ident}:\n{claim}");
    }
    assert_eq!(
        out.line("no area:"),
        "no area: 3 (no ancestor 2, several places 1)"
    );
    assert_eq!(out.line("areas:"), "areas: 3 paths; filed 3 of 6 drafted");
    // The hierarchy line names how many drafts the ancestors were carried for.
    let hierarchy = out.line("hierarchy");
    assert!(
        hierarchy
            .ends_with("— the ancestors of 3 drafted objects are carried as the area (--areas)"),
        "{hierarchy}"
    );
    assert_adds_up(&out, "areas, no area");
}

#[test]
fn the_hierarchy_line_does_not_say_carried_when_no_draft_was_filed() {
    // SYNTHETIC: the one requirement sits directly under the specification, so nothing is filed.
    let mut doc = Doc::new().object(req_xhtml("r", &div("The r shall hold.")));
    doc.specs = specification("S", &node("r", ""));
    let fx = synthetic_fixture("req-reqif-areas-hierarchy-none", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas"]));
    out.assert_code(0, "nothing filed");
    assert_eq!(out.line("areas:"), "areas: 0 paths; filed 0 of 1 drafted");
    let hierarchy = out.line("hierarchy");
    assert!(
        hierarchy.ends_with("— the structure is not carried") && !hierarchy.contains("area"),
        "{hierarchy}"
    );
}

#[test]
fn a_collision_above_a_draft_that_is_filed_nowhere_is_still_a_problem() {
    // SYNTHETIC: `r` is reached twice (so it has no area), once under `Sec.A` and once under
    // `sec_a`, two identifiers that make one key. Nothing is filed, and it is exit 1 all the same.
    let mut doc = Doc::new()
        .object(obj("Sec.A", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("A"))]))
        .object(obj("sec_a", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("B"))]))
        .object(req_xhtml("r", &div("The r shall hold.")));
    doc.specs = specification(
        "S",
        &format!(
            "{}{}",
            node("Sec.A", &node("r", "")),
            node("sec_a", &node("r", ""))
        ),
    );
    let fx = synthetic_fixture("req-reqif-areas-collision-unfiled", &doc);
    let before = fx.claims();
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(1, "a collision above an unfiled draft");
    assert!(
        out.stderr.contains("Sec.A") && out.stderr.contains("sec_a"),
        "names both identifiers: {}",
        out.stderr
    );
    assert_eq!(fx.claims(), before, "nothing is drafted");
    assert!(
        !fx.path().join(".req/areas").exists(),
        "no registry is written"
    );

    // The depth cut applies to this check as to the rest: the same two identifiers one level
    // down, with `--area-depth 1` stopping above them, collide nowhere.
    let mut doc = Doc::new()
        .object(obj("top", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("T"))]))
        .object(obj("Sec.A", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("A"))]))
        .object(obj("sec_a", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("B"))]))
        .object(req_xhtml("r", &div("The r shall hold.")));
    doc.specs = specification(
        "S",
        &node(
            "top",
            &format!(
                "{}{}",
                node("Sec.A", &node("r", "")),
                node("sec_a", &node("r", ""))
            ),
        ),
    );
    let fx = synthetic_fixture("req-reqif-areas-collision-cut", &doc);
    let out = import(
        fx.path(),
        &plus(base(&REQ_ARGS), &["--areas", "--area-depth", "1"]),
    );
    out.assert_code(0, "the colliding identifiers are below the depth");
    let full = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas"]));
    full.assert_code(1, "without the cut they collide");
}

#[test]
fn an_ancestor_that_names_no_object_files_nothing_and_is_counted_by_reason() {
    // SYNTHETIC: a hierarchy node with no SPEC-OBJECT-REF above `r`; `s` sits under a real heading.
    let mut doc = Doc::new()
        .object(obj("h", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("H"))]))
        .object(req_xhtml("r", &div("The r shall hold.")))
        .object(req_xhtml("s", &div("The s shall hold.")));
    doc.specs = specification(
        "S",
        &format!(
            "<SPEC-HIERARCHY IDENTIFIER=\"HN-unnamed\"><CHILDREN>{}</CHILDREN></SPEC-HIERARCHY>{}",
            node("r", ""),
            node("h", &node("s", ""))
        ),
    );
    let fx = synthetic_fixture("req-reqif-areas-unnamed", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(0, "an ancestor that names no object is not exit 3");
    let r = claim_of(&fx, &out, "r");
    assert_eq!(schema_of(&r), "schema: req.claim/2", "{r}");
    assert!(area_of(&r).is_none(), "{r}");
    assert_eq!(
        area_of(&claim_of(&fx, &out, "s")),
        Some("area: h"),
        "{}",
        out.stdout
    );
    assert_eq!(out.line("no area:"), "no area: 1 (unnamed ancestor 1)");
    assert_adds_up(&out, "unnamed ancestor");
}

/// The claims write cleans up after anything that ends it, an interrupt included: a partial set
/// of claims beside a registry that was rolled back is a half-run. Driven through the tool's own
/// function, since a signal cannot be aimed at one write.
#[test]
fn an_interrupt_during_the_claims_write_removes_what_was_written() {
    let fx = Fixture::new("req-reqif-areas-interrupt");
    let out = Command::new(python3())
        .arg("-I")
        .arg("-c")
        .arg(INTERRUPT_SCRIPT)
        .arg(script())
        .arg(fx.path())
        .output()
        .expect("runs python3");
    outcome(&out).assert_code(
        0,
        "the interrupt: 3 = no interrupt, 4 = a claim was left behind",
    );
}

const INTERRUPT_SCRIPT: &str = r#"
import importlib.util, pathlib, sys
spec = importlib.util.spec_from_file_location("tool", sys.argv[1])
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)
directory = pathlib.Path(sys.argv[2])

class Interrupt:
    def __fspath__(self):
        raise KeyboardInterrupt

first = directory / "REQ-0100.yml"
try:
    tool.write_claims([first, Interrupt()], ["a\n", "b\n"])
except KeyboardInterrupt:
    pass
else:
    sys.exit(3)
sys.exit(4 if first.exists() else 0)
"#;

#[test]
fn an_object_in_two_specifications_has_no_area_and_the_reader_picks_no_place() {
    // SYNTHETIC: `r` sits under heading `h-a` in one specification and under `h-b` in another.
    let mut doc = Doc::new()
        .object(obj("h-a", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("A"))]))
        .object(obj("h-b", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("B"))]))
        .object(req_xhtml("r", &div("The r shall hold.")))
        .object(req_xhtml("s", &div("The s shall hold.")));
    doc.specs = format!(
        "{}{}",
        specification(
            "One",
            &node("h-a", &format!("{}{}", node("r", ""), node("s", "")))
        ),
        specification("Two", &node("h-b", &node("r", "")))
    );
    let fx = synthetic_fixture("req-reqif-areas-two-specs", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(0, "two specifications");
    let r = claim_of(&fx, &out, "r");
    let s = claim_of(&fx, &out, "s");
    assert!(area_of(&r).is_none(), "{r}");
    assert_eq!(area_of(&s), Some("area: h-a"), "{s}");
    assert_eq!(out.line("no area:"), "no area: 1 (several places 1)");
}

#[test]
fn depth_keeps_only_the_outermost_segments() {
    let fx = Fixture::with_public("req-reqif-areas-depth", AREAS_SAMPLE);
    let out = sample_run(&fx, &["--area-depth", "1", "--write"]);
    out.assert_code(0, "--area-depth 1");
    for ident in ["R-1", "R-2", "R-3"] {
        let claim = claim_of(&fx, &out, ident);
        assert_eq!(area_of(&claim), Some("area: h-0"), "{ident}:\n{claim}");
    }
    assert_eq!(out.line("areas:"), "areas: 1 paths; filed 3 of 6 drafted");
    assert_eq!(fx.read(".req/areas"), "h-0 Diagnostics\n");

    // A depth past the deepest chain changes nothing.
    let fx = Fixture::with_public("req-reqif-areas-depth-deep", AREAS_SAMPLE);
    let out = sample_run(&fx, &["--area-depth", "9", "--write"]);
    out.assert_code(0, "--area-depth 9");
    assert_eq!(fx.read(".req/areas"), SAMPLE_REGISTRY);
}

#[test]
fn the_key_rule_lowercases_collapses_runs_and_strips_the_ends() {
    // SYNTHETIC: the heading's IDENTIFIER is `_Sec.A 12`.
    let mut doc = Doc::new()
        .object(obj(
            "_Sec.A 12",
            "T-NOTE",
            &[text_xhtml("AD-NTEXT", &div("A"))],
        ))
        .object(req_xhtml("r", &div("The r shall hold.")));
    doc.specs = specification("S", &node("_Sec.A 12", &node("r", "")));
    let fx = synthetic_fixture("req-reqif-areas-key", &doc);
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(0, "key rule");
    assert_eq!(
        area_of(&claim_of(&fx, &out, "r")),
        Some("area: sec-a-12"),
        "{}",
        out.stdout
    );
}

#[test]
fn two_identifiers_that_make_one_path_are_a_problem_naming_both_and_nothing_is_written() {
    // SYNTHETIC: `Sec.A` and `sec_a` are two headings, one key.
    let mut doc = Doc::new()
        .object(obj("Sec.A", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("A"))]))
        .object(obj("sec_a", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("B"))]))
        .object(req_xhtml("r1", &div("The r1 shall hold.")))
        .object(req_xhtml("r2", &div("The r2 shall hold.")));
    doc.specs = specification(
        "S",
        &format!(
            "{}{}",
            node("Sec.A", &node("r1", "")),
            node("sec_a", &node("r2", ""))
        ),
    );
    let fx = synthetic_fixture("req-reqif-areas-collision", &doc);
    let before = fx.claims();
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(1, "a key collision");
    assert!(
        out.stderr.contains("Sec.A")
            && out.stderr.contains("sec_a")
            && out.stderr.contains("sec-a"),
        "names both identifiers and the path: {}",
        out.stderr
    );
    assert_eq!(fx.claims(), before, "nothing is drafted");
    assert!(
        !fx.path().join(".req/areas").exists(),
        "no registry is written"
    );
}

#[test]
fn an_identifier_that_leaves_no_key_is_a_problem_naming_it() {
    // SYNTHETIC: the heading's IDENTIFIER is only punctuation.
    let mut doc = Doc::new()
        .object(obj("___", "T-NOTE", &[text_xhtml("AD-NTEXT", &div("A"))]))
        .object(req_xhtml("r", &div("The r shall hold.")));
    doc.specs = specification("S", &node("___", &node("r", "")));
    let fx = synthetic_fixture("req-reqif-areas-empty-key", &doc);
    let before = fx.claims();
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(1, "an identifier with no key");
    assert!(out.stderr.contains("___"), "{}", out.stderr);
    assert_eq!(fx.claims(), before);
}

#[test]
fn an_ancestor_the_file_does_not_hold_is_unverifiable() {
    // SYNTHETIC: the hierarchy names a parent object that no SPEC-OBJECT declares.
    let mut doc = Doc::new().object(req_xhtml("r", &div("The r shall hold.")));
    doc.specs = specification("S", &node("GHOST", &node("r", "")));
    let fx = synthetic_fixture("req-reqif-areas-ghost", &doc);
    let before = fx.claims();
    let out = import(fx.path(), &plus(base(&REQ_ARGS), &["--areas", "--write"]));
    out.assert_code(3, "an unresolved ancestor");
    assert!(out.stderr.starts_with("[UNVERIFIABLE]"), "{}", out.stderr);
    assert!(out.stderr.contains("GHOST"), "{}", out.stderr);
    assert_eq!(fx.claims(), before);
}

#[test]
fn the_label_comes_from_the_label_attribute_then_the_id_attribute_then_the_segment() {
    // (a) both flags: a heading's text; the parent requirement has no such attribute, so its ID.
    let fx = Fixture::with_public("req-reqif-areas-label-a", AREAS_SAMPLE);
    let out = sample_run(&fx, &[]);
    out.assert_code(0, "label, both flags");
    assert_eq!(
        proposal_lines(&out),
        vec![
            "  h-0/h-1 Démarrage 起動",
            "  h-0/h-1/r-1 SW-1",
            "  h-0 Diagnostics"
        ],
        "{}",
        out.stdout
    );
    assert!(!fx.path().join(".req").exists(), "a dry run writes nothing");

    // (b) the label attribute alone: the requirement falls back to its segment.
    let fx = Fixture::with_public("req-reqif-areas-label-b", AREAS_SAMPLE);
    let mut args = base(&["--type", "Requirement", "--text-attr", "ReqIF.Text"]);
    args.extend(strings(&[
        "--areas",
        "--area-label-attr",
        "ReqIF.ChapterName",
        "--write",
    ]));
    import(fx.path(), &args).assert_code(0, "label, attribute alone");
    assert_eq!(
        fx.read(".req/areas"),
        "h-0/h-1 Démarrage 起動\nh-0/h-1/r-1 r-1\nh-0 Diagnostics\n"
    );

    // (c) the id attribute alone: every ancestor's own ID.
    let fx = Fixture::with_public("req-reqif-areas-label-c", AREAS_SAMPLE);
    import(
        fx.path(),
        &plus(base(&SAMPLE_ARGS), &["--areas", "--write"]),
    )
    .assert_code(0, "label, id attribute alone");
    assert_eq!(
        fx.read(".req/areas"),
        "h-0/h-1 D-1\nh-0/h-1/r-1 SW-1\nh-0 D-0\n"
    );

    // (d) neither: the last segment of each path.
    let fx = Fixture::with_public("req-reqif-areas-label-d", AREAS_SAMPLE);
    let args = plus(
        base(&["--type", "Requirement", "--text-attr", "ReqIF.Text"]),
        &["--areas", "--write"],
    );
    import(fx.path(), &args).assert_code(0, "label, neither flag");
    assert_eq!(
        fx.read(".req/areas"),
        "h-0/h-1 h-1\nh-0/h-1/r-1 r-1\nh-0 h-0\n"
    );
}

#[test]
fn a_non_ascii_label_survives_byte_for_byte() {
    let fx = Fixture::with_public("req-reqif-areas-label-utf8", AREAS_SAMPLE);
    sample_run(&fx, &["--write"]).assert_code(0, "a non-ASCII label");
    let bytes = fs::read(fx.path().join(".req/areas")).expect("the registry");
    let wanted = "Démarrage 起動".as_bytes();
    assert!(
        bytes.windows(wanted.len()).any(|window| window == wanted),
        "the label's UTF-8 bytes are in the registry: {bytes:?}"
    );
}

#[test]
fn a_label_attribute_no_type_defines_is_a_problem_and_one_matching_two_definitions_is_unverifiable()
{
    let fx = Fixture::with_public("req-reqif-areas-label-unknown", AREAS_SAMPLE);
    let before = fx.claims();
    let out = import(
        fx.path(),
        &plus(
            base(&SAMPLE_ARGS),
            &[
                "--areas",
                "--area-label-attr",
                "No Such Attribute",
                "--write",
            ],
        ),
    );
    out.assert_code(1, "an unknown label attribute");
    assert!(out.stderr.contains("No Such Attribute"), "{}", out.stderr);
    assert_eq!(fx.claims(), before);

    // SYNTHETIC: the parent requirement's type defines `Dup` twice.
    let mut doc = Doc::new()
        .object(req_xhtml("p", &div("The p shall hold.")))
        .object(req_xhtml("c", &div("The c shall hold.")));
    doc.extra_defs = "<ATTRIBUTE-DEFINITION-STRING IDENTIFIER=\"AD-D1\" LONG-NAME=\"Dup\"><TYPE><DATATYPE-DEFINITION-STRING-REF>DT-S</DATATYPE-DEFINITION-STRING-REF></TYPE></ATTRIBUTE-DEFINITION-STRING><ATTRIBUTE-DEFINITION-STRING IDENTIFIER=\"AD-D2\" LONG-NAME=\"Dup\"><TYPE><DATATYPE-DEFINITION-STRING-REF>DT-S</DATATYPE-DEFINITION-STRING-REF></TYPE></ATTRIBUTE-DEFINITION-STRING>".to_string();
    doc.specs = specification("S", &node("p", &node("c", "")));
    let fx = synthetic_fixture("req-reqif-areas-label-two", &doc);
    let before = fx.claims();
    let out = import(
        fx.path(),
        &plus(
            base(&REQ_ARGS),
            &["--areas", "--area-label-attr", "Dup", "--write"],
        ),
    );
    out.assert_code(3, "a label attribute with two definitions");
    assert!(
        out.stderr.contains("Dup") && out.stderr.contains("two"),
        "{}",
        out.stderr
    );
    assert_eq!(fx.claims(), before);
}

#[test]
fn a_write_creates_the_registry_in_source_order_and_a_dry_run_reports_the_same_lines() {
    let fx = Fixture::with_public("req-reqif-areas-registry-create", AREAS_SAMPLE);
    let dry = sample_run(&fx, &[]);
    dry.assert_code(0, "dry run");
    assert_eq!(
        dry.line("registry:"),
        "registry: 3 lines to add, 0 already declared"
    );
    assert!(!fx.path().join(".req/areas").exists());
    assert_eq!(fx.claims().len(), 1, "only the corpus the init made");

    let out = sample_run(&fx, &["--write"]);
    out.assert_code(0, "write");
    // The hierarchy's reading order (R-1's chain, R-2's, then R-3's), not the file's object order.
    assert_eq!(fx.read(".req/areas"), SAMPLE_REGISTRY);
    assert_eq!(proposal_lines(&out), proposal_lines(&dry));
    // Each proposed line is `<path> <label>` and pastes into `.req/areas` as it is.
    let pasted: String = proposal_lines(&dry)
        .iter()
        .map(|line| format!("{}\n", line.trim()))
        .collect();
    assert_eq!(pasted, SAMPLE_REGISTRY);
    assert_adds_up(&out, "registry created");
}

#[test]
fn a_declared_path_is_left_alone_whatever_its_label_and_the_rest_is_appended() {
    let fx = Fixture::with_public("req-reqif-areas-registry-existing", AREAS_SAMPLE);
    // No trailing newline, a comment, and a label that differs from the source's.
    fx.write(".req/areas", "# my areas\nh-0 Custom name");
    let out = sample_run(&fx, &["--write"]);
    out.assert_code(0, "an existing registry");
    assert_eq!(
        fx.read(".req/areas"),
        "# my areas\nh-0 Custom name\nh-0/h-1 Démarrage 起動\nh-0/h-1/r-1 SW-1\n"
    );
    assert_eq!(
        out.line("registry:"),
        "registry: 2 lines to add, 1 already declared"
    );

    // All three declared: the file is not touched, byte for byte.
    let fx = Fixture::with_public("req-reqif-areas-registry-full", AREAS_SAMPLE);
    let declared = "h-0 One\nh-0/h-1 Two\nh-0/h-1/r-1 Three\n";
    fx.write(".req/areas", declared);
    let out = sample_run(&fx, &["--write"]);
    out.assert_code(0, "a registry that declares everything");
    assert_eq!(fx.read(".req/areas"), declared);
    assert_eq!(
        out.line("registry:"),
        "registry: 0 lines to add, 3 already declared"
    );
}

#[test]
fn a_registry_that_is_malformed_before_the_run_is_unverifiable_and_names_the_line() {
    let fx = Fixture::with_public("req-reqif-areas-registry-bad", AREAS_SAMPLE);
    fx.write(".req/areas", "h-0 Fine\nNot A Path x\n");
    let before = fx.claims();
    let out = sample_run(&fx, &["--write"]);
    out.assert_code(3, "a malformed registry");
    assert!(out.stderr.starts_with("[UNVERIFIABLE]"), "{}", out.stderr);
    assert!(out.stderr.contains(".req/areas:2"), "{}", out.stderr);
    assert_eq!(fx.claims(), before);
    assert_eq!(fx.read(".req/areas"), "h-0 Fine\nNot A Path x\n");
}

/// Makes a directory writable again when dropped, so a failed test does not leave a fixture the
/// harness cannot remove.
#[cfg(unix)]
struct Unlock(PathBuf);

#[cfg(unix)]
impl Drop for Unlock {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

/// The claims directory cannot be written: the registry is written first, so a failure then comes
/// *after* it changed, and its earlier bytes must come back (or the file go, when the run made it).
#[cfg(unix)]
#[test]
fn a_failure_after_the_registry_was_written_puts_it_back() {
    use std::os::unix::fs::PermissionsExt;

    for existing in [Some("# mine\nh-0 Custom\n"), None] {
        let fx = Fixture::with_public("req-reqif-areas-restore", AREAS_SAMPLE);
        if let Some(text) = existing {
            fx.write(".req/areas", text);
        }
        let claims = fx.path().join("claims");
        fs::set_permissions(&claims, fs::Permissions::from_mode(0o555)).expect("chmod");
        let _unlock = Unlock(claims.clone());
        assert!(
            fs::write(claims.join(".probe"), "x").is_err(),
            "this test needs a user the permission bits bind (not root): claims/ is still writable"
        );
        let out = sample_run(&fx, &["--write"]);
        out.assert_code(3, "claims/ cannot be written");
        assert!(out.stderr.starts_with("[UNVERIFIABLE]"), "{}", out.stderr);
        assert!(
            out.stderr.contains("cannot write"),
            "the failure is the claims write, after the registry's: {}",
            out.stderr
        );
        match existing {
            Some(text) => assert_eq!(fx.read(".req/areas"), text, "its earlier bytes are back"),
            None => assert!(
                !fx.path().join(".req/areas").exists(),
                "the file the run created is removed"
            ),
        }
    }
}

#[test]
fn the_corpus_the_run_leaves_loads_and_check_finds_no_undeclared_area() {
    let fx = Fixture::with_public("req-reqif-areas-loads", AREAS_SAMPLE);
    sample_run(&fx, &["--write"]).assert_code(0, "write");
    req(fx.path(), &["canon"]).assert_code(0, "req canon on the corpus the run leaves");

    let check = req(fx.path(), &["check", "--format", "json"]);
    let doc: serde_json::Value = serde_json::from_str(&check.stdout).unwrap_or_else(|e| {
        panic!(
            "req check --format json ({e}):\n{}\n{}",
            check.stdout, check.stderr
        )
    });
    let step = doc["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|s| s["kind"] == "completeness")
        .expect("a completeness step");
    let undeclared = step["completeness"]["undeclared_area"]
        .as_array()
        .unwrap_or_else(|| panic!("undeclared_area is a list when `.req/areas` exists:\n{step}"));
    assert!(undeclared.is_empty(), "{undeclared:?}");

    // The area is what `req canon` shows, and only the filed drafts have one.
    let canon = req(fx.path(), &["canon"]);
    assert!(canon.stdout.contains("h-0/h-1/r-1"), "{}", canon.stdout);
}

#[cfg(unix)]
#[test]
fn areas_need_a_req_that_reports_claim_schema_3() {
    let fx = Fixture::with_public("req-reqif-areas-old-req", AREAS_SAMPLE);
    let bin = fake_req_reporting(&fx, "req.claim/2");
    let before = fx.claims();
    let out = import_in(
        fx.path(),
        &plus(base(&SAMPLE_ARGS), &["--areas", "--write"]),
        &Env {
            path_first: Some(&bin),
            ..Env::default()
        },
    );
    out.assert_code(3, "--areas against a req reporting req.claim/2");
    assert!(
        out.stderr.starts_with("[UNVERIFIABLE]")
            && out.stderr.contains("req.claim/2")
            && out.stderr.contains("req.claim/3"),
        "{}",
        out.stderr
    );
    assert_eq!(fx.claims(), before);
    assert!(!fx.path().join(".req/areas").exists());

    // Without the flag the same req is accepted, as before.
    let out = import_in(
        fx.path(),
        &plus(base(&SAMPLE_ARGS), &["--write"]),
        &Env {
            path_first: Some(&bin),
            ..Env::default()
        },
    );
    out.assert_code(0, "no --areas against a req reporting req.claim/2");

    // And a req that reports req.claim/3 takes the flag.
    let fx = Fixture::with_public("req-reqif-areas-new-req", AREAS_SAMPLE);
    let bin = fake_req_reporting(&fx, "req.claim/3");
    let out = import_in(
        fx.path(),
        &plus(base(&SAMPLE_ARGS), &["--areas", "--write"]),
        &Env {
            path_first: Some(&bin),
            ..Env::default()
        },
    );
    out.assert_code(0, "--areas against a req reporting req.claim/3");
}

#[test]
fn a_malformed_area_option_is_a_usage_error() {
    let fx = Fixture::with_public("req-reqif-areas-usage", AREAS_SAMPLE);
    let before = fx.claims();
    for extra in [
        vec!["--areas", "--area-depth", "0"],
        vec!["--areas", "--area-depth", "-1"],
        vec!["--areas", "--area-depth", "two"],
        vec!["--areas", "--area-label-attr", ""],
        vec!["--area-depth", "1"],
        vec!["--area-label-attr", "ReqIF.ChapterName"],
    ] {
        let out = import(fx.path(), &plus(base(&SAMPLE_ARGS), &extra));
        out.assert_code(2, &format!("{extra:?}"));
    }
    let out = import(
        fx.path(),
        &strings(&["--from", "module.reqif", "--list", "--areas"]),
    );
    out.assert_code(2, "--list takes no --areas");
    assert_eq!(fx.claims(), before);

    // The control: the same options, well formed, are a run.
    let out = import(
        fx.path(),
        &plus(base(&SAMPLE_ARGS), &["--areas", "--area-depth", "1"]),
    );
    out.assert_code(0, "well-formed area options");
}

#[test]
fn the_self_check_holds_the_key_rule() {
    let bare = Fixture::new("req-reqif-areas-selfcheck");
    let out = Command::new(python3())
        .arg(script())
        .arg("--self-check")
        .current_dir(bare.path())
        .env("PATH", "")
        .env_remove("REQ_ROOT")
        .output()
        .expect("runs tools/reqif_import.py --self-check");
    let out = outcome(&out);
    out.assert_code(0, "--self-check");
    let rows: Vec<&str> = out
        .stdout
        .lines()
        .filter(|line| line.trim_start().starts_with("ok") && line.contains(" key: "))
        .collect();
    assert!(
        rows.len() >= 6,
        "the self-check holds the key rule's cases ({} rows):\n{}",
        rows.len(),
        out.stdout
    );
    assert!(out.stdout.contains("0 failure(s)"), "{}", out.stdout);
}

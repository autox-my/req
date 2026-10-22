//! The JUnit reader: what a test report says, and what it refuses to say.
//!
//! This is a **hand-written reader for nextest's JUnit subset** — no XML crate and no
//! `Cargo.lock` change (the lockfile is an identity-path pin; a dependency bump is a
//! Tier-1 rules bump, and a test report is not worth one). The subset is what
//! `cargo nextest run --profile ci` writes: a `<testsuites>` root holding `<testsuite>`
//! elements, each holding `<testcase name="…" classname="…">` with an optional
//! `<failure>` / `<error>` / `<skipped>` child. `<system-out>` / `<system-err>` are
//! ignored wholesale, as are comments, processing instructions and inter-element
//! whitespace. A single `<testsuite>` root is accepted too; a root of any other name is
//! refused rather than read as an empty report.
//!
//! # The tri-state
//!
//! A report is in exactly one of three states, and the reader keeps them apart because
//! the gate gives each a different answer:
//!
//!   * **parsed** — [`Report::parse`] returned a report; a test is either present with an
//!     outcome or absent from it;
//!   * **unparsable** — `Err(reason)`, never an empty report: a malformed file is
//!     UNVERIFIABLE, not "no test failed";
//!   * **no test cases** — `Ok(report)` with [`Report::is_empty`] true (the
//!     `<testsuites/>` a zero-test run writes). This is a distinct state on purpose: it
//!     is neither a pass nor a parse error.
//!
//! # Identity
//!
//! A test's id is `classname` + `::` + `name` — the pair nextest prints. That id is the
//! identity a claim's `tests:` entry names. It is deliberately *not* on a claim's
//! address path: the address is a content address over canonical meaning, and a test
//! rename must never move it. That is also why the reader refuses a report carrying the
//! same id twice with different outcomes: pick one silently and a rename or a flake would
//! read as a decided result. Identical duplicate rows dedupe.
//!
//! # A skipped test is never a pass
//!
//! [`TestOutcome`] has three variants and the reader never folds two of them together.
//! What a skipped test *means* for a requirement is the caller's judgement; losing the
//! distinction here would take that judgement away. (nextest emits `<skipped>` rows only
//! when the profile opts in — `report-skipped` under `[profile.ci.junit]` — and omits
//! ignored tests entirely otherwise, which the gate reads as absent, never as a pass.)
//!
//! The parser is total on `&str`: every slice is taken with `get`, and no input panics.
//! The `tests` / `failures` / `skipped` counters on the elements are not trusted — the
//! `<testcase>` elements are the report.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::Path;

/// What the report says about one test. Three states, never two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    Passed,
    Failed,
    Skipped,
}

/// The parsed report: a deterministic map from test id (`classname::name`) to outcome.
///
/// Backed by a `BTreeMap`, so nothing observable depends on insertion order or on a hash
/// seed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    outcomes: BTreeMap<String, TestOutcome>,
}

impl Report {
    /// Parse nextest's JUnit subset. `Err(reason)` for malformed input — never a silent
    /// empty report.
    pub fn parse(text: &str) -> Result<Report, String> {
        let mut scanner = Scanner::new(text);
        let mut report = Report::default();

        let (root, self_closing) = match scanner.next()? {
            Some(Event::Start {
                name, self_closing, ..
            }) => (name, self_closing),
            Some(Event::End { name }) => {
                return Err(format!(
                    "unexpected closing tag `</{name}>` before any root element"
                ));
            }
            None => {
                return Err("no root element: expected `<testsuites>` or `<testsuite>`".to_string());
            }
        };
        if root != "testsuites" && root != "testsuite" {
            return Err(format!(
                "unexpected root element `<{root}>`: expected `<testsuites>` or `<testsuite>`"
            ));
        }
        if !self_closing {
            parse_container(&mut scanner, &root, &mut report)?;
        }
        if let Some(event) = scanner.next()? {
            let what = match event {
                Event::Start { name, .. } => format!("`<{name}>`"),
                Event::End { name } => format!("`</{name}>`"),
            };
            return Err(format!("content after the root element: {what}"));
        }
        Ok(report)
    }

    /// Read and parse a report. An I/O failure names the path it could not read.
    pub fn load(path: &Path) -> Result<Report, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read test report `{}`: {error}", path.display()))?;
        Report::parse(&text)
            .map_err(|reason| format!("cannot parse test report `{}`: {reason}", path.display()))
    }

    /// No testcase elements at all — a distinct state the gate reports as UNVERIFIABLE.
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }

    /// Look a test up by its stable id (`classname::name`). `None` = absent from the
    /// report.
    pub fn outcome(&self, id: &str) -> Option<TestOutcome> {
        self.outcomes.get(id).copied()
    }

    /// How many distinct tests the report names.
    pub fn test_count(&self) -> usize {
        self.outcomes.len()
    }
}

/// Walk the children of `<testsuites>` or `<testsuite>`. Recursion is bounded at two
/// levels: a `<testsuite>` nested inside a `<testsuite>` is not a container we read.
fn parse_container(
    scanner: &mut Scanner<'_>,
    container: &str,
    report: &mut Report,
) -> Result<(), String> {
    loop {
        match scanner.next()? {
            None => return Err(format!("unclosed `<{container}>`")),
            Some(Event::End { name }) if name == container => return Ok(()),
            Some(Event::End { name }) => {
                return Err(format!(
                    "unexpected closing tag `</{name}>` inside `<{container}>`"
                ));
            }
            Some(Event::Start {
                name,
                attrs,
                self_closing,
            }) => {
                if name == "testcase" {
                    parse_testcase(scanner, &attrs, self_closing, report)?;
                } else if container == "testsuites" && name == "testsuite" {
                    if !self_closing {
                        parse_container(scanner, "testsuite", report)?;
                    }
                } else if !self_closing {
                    // Properties, captured output, anything else: its content cannot
                    // become a test row, so skip it without tokenizing its text.
                    scanner.skip_element(&name)?;
                }
            }
        }
    }
}

fn parse_testcase(
    scanner: &mut Scanner<'_>,
    attrs: &[(String, String)],
    self_closing: bool,
    report: &mut Report,
) -> Result<(), String> {
    let name = required_attribute(attrs, "name")?;
    let classname = required_attribute(attrs, "classname")?;
    // Presence is not identity: `name=""` would make the id `classname::`, a plausible-looking
    // string no claim could ever link to intentionally. An id that cannot be written is not a
    // test report entry.
    if name.is_empty() || classname.is_empty() {
        return Err(format!(
            "`<testcase>` has an empty {} attribute: the id is `classname::name` and an empty \
             part names no test",
            if name.is_empty() { "name" } else { "classname" }
        ));
    }
    let id = format!("{classname}::{name}");
    let mut outcome = TestOutcome::Passed;

    if !self_closing {
        loop {
            match scanner.next()? {
                None => return Err(format!("unclosed `<testcase name=\"{name}\">`")),
                Some(Event::End { name: end }) if end == "testcase" => break,
                Some(Event::End { name: end }) => {
                    return Err(format!(
                        "unexpected closing tag `</{end}>` inside `<testcase name=\"{name}\">`"
                    ));
                }
                Some(Event::Start {
                    name: child,
                    self_closing: child_self_closing,
                    ..
                }) => {
                    match child.as_str() {
                        "failure" | "error" => outcome = TestOutcome::Failed,
                        // A failure outranks a skip in the same case: the test ran and
                        // failed, whatever else the report also says about it.
                        "skipped" if outcome != TestOutcome::Failed => {
                            outcome = TestOutcome::Skipped;
                        }
                        _ => {}
                    }
                    if !child_self_closing {
                        scanner.skip_element(&child)?;
                    }
                }
            }
        }
    }

    insert(&mut report.outcomes, &id, outcome)
}

/// Record one row. Identical duplicates dedupe; a conflicting duplicate is an error —
/// never a silent tie-break.
fn insert(
    outcomes: &mut BTreeMap<String, TestOutcome>,
    id: &str,
    outcome: TestOutcome,
) -> Result<(), String> {
    match outcomes.entry(id.to_string()) {
        Entry::Vacant(slot) => {
            slot.insert(outcome);
            Ok(())
        }
        Entry::Occupied(slot) if *slot.get() == outcome => Ok(()),
        Entry::Occupied(slot) => Err(format!(
            "duplicate test id `{id}` with conflicting outcomes ({:?} and {outcome:?}); \
             a rename or a flake must not read as a chosen result",
            slot.get()
        )),
    }
}

fn required_attribute<'a>(attrs: &'a [(String, String)], key: &str) -> Result<&'a str, String> {
    attrs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .ok_or_else(|| format!("`<testcase>` has no `{key}` attribute"))
}

/// The event stream: only element boundaries matter; text between them is ignored.
enum Event {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    End {
        name: String,
    },
}

/// A byte-oriented scanner over an `&str`. Every slice is taken with `get`, and every
/// position it hands out sits on a character boundary (delimiters, names and quotes are
/// ASCII).
struct Scanner<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { text, pos: 0 }
    }

    fn next(&mut self) -> Result<Option<Event>, String> {
        loop {
            let rest = self.rest()?;
            let Some(offset) = rest.find('<') else {
                self.pos = self.text.len();
                return Ok(None);
            };
            self.pos += offset;
            let Some(&byte) = self.text.as_bytes().get(self.pos + 1) else {
                return Err("unterminated `<` at the end of the report".to_string());
            };
            match byte {
                b'?' => self.skip_past_next("?>", "processing instruction")?,
                b'!' => self.skip_declaration()?,
                b'/' => return Ok(Some(self.read_end_tag()?)),
                _ if is_name_start(byte) => return Ok(Some(self.read_start_tag()?)),
                _ => {
                    return Err(format!(
                        "stray `<` at byte {}: not the start of a tag",
                        self.pos
                    ));
                }
            }
        }
    }

    fn rest(&self) -> Result<&'a str, String> {
        self.text
            .get(self.pos..)
            .ok_or_else(|| "parser position is not a character boundary".to_string())
    }

    fn skip_past_next(&mut self, delimiter: &str, what: &str) -> Result<(), String> {
        let rest = self.rest()?;
        let Some(offset) = rest.find(delimiter) else {
            return Err(format!("unterminated {what}"));
        };
        self.pos += offset + delimiter.len();
        Ok(())
    }

    fn skip_declaration(&mut self) -> Result<(), String> {
        let rest = self.rest()?;
        if rest.starts_with("<!--") {
            self.skip_past_next("-->", "comment")
        } else if rest.starts_with("<![CDATA[") {
            self.skip_past_next("]]>", "CDATA section")
        } else {
            Err(format!(
                "unsupported declaration at byte {}: `<!DOCTYPE>` and friends are outside \
                 nextest's JUnit subset",
                self.pos
            ))
        }
    }

    fn read_start_tag(&mut self) -> Result<Event, String> {
        self.pos += 1; // the '<'
        let name = self.read_name()?;
        let mut attrs = Vec::new();
        loop {
            self.skip_whitespace();
            let Some(&byte) = self.text.as_bytes().get(self.pos) else {
                return Err(format!("end of input inside the opening tag `<{name}>`"));
            };
            match byte {
                b'>' => {
                    self.pos += 1;
                    return Ok(Event::Start {
                        name,
                        attrs,
                        self_closing: false,
                    });
                }
                b'/' => {
                    if self.text.as_bytes().get(self.pos + 1) == Some(&b'>') {
                        self.pos += 2;
                        return Ok(Event::Start {
                            name,
                            attrs,
                            self_closing: true,
                        });
                    }
                    return Err(format!("`/` inside `<{name}>` is not followed by `>`"));
                }
                _ => {
                    if !is_name_start(byte) {
                        return Err(format!(
                            "malformed attribute in `<{name}>`: expected an attribute name"
                        ));
                    }
                    let attr = self.read_name()?;
                    self.skip_whitespace();
                    if self.text.as_bytes().get(self.pos) != Some(&b'=') {
                        return Err(format!("attribute `{attr}` in `<{name}>` has no `=`"));
                    }
                    self.pos += 1;
                    self.skip_whitespace();
                    let Some(&quote) = self.text.as_bytes().get(self.pos) else {
                        return Err(format!("attribute `{attr}` in `<{name}>` has no value"));
                    };
                    if quote != b'"' && quote != b'\'' {
                        return Err(format!("attribute `{attr}` in `<{name}>` is not quoted"));
                    }
                    self.pos += 1;
                    let value_start = self.pos;
                    while let Some(&current) = self.text.as_bytes().get(self.pos) {
                        if current == quote {
                            break;
                        }
                        self.pos += 1;
                    }
                    if self.text.as_bytes().get(self.pos) != Some(&quote) {
                        return Err(format!(
                            "unterminated value for attribute `{attr}` in `<{name}>`"
                        ));
                    }
                    let raw = self
                        .text
                        .get(value_start..self.pos)
                        .ok_or_else(|| format!("attribute `{attr}` in `<{name}>` is not UTF-8"))?;
                    self.pos += 1;
                    let value = decode_entities(raw)
                        .map_err(|reason| format!("attribute `{attr}` in `<{name}>`: {reason}"))?;
                    attrs.push((attr, value));
                }
            }
        }
    }

    fn read_end_tag(&mut self) -> Result<Event, String> {
        self.pos += 2; // the "</"
        let name = self.read_name()?;
        self.skip_whitespace();
        if self.text.as_bytes().get(self.pos) == Some(&b'>') {
            self.pos += 1;
            Ok(Event::End { name })
        } else {
            Err(format!("unterminated closing tag `</{name}`"))
        }
    }

    /// Read an element or attribute name (ASCII, as XML names in this subset are).
    fn read_name(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(&byte) = self.text.as_bytes().get(self.pos) {
            if !is_name_byte(byte) {
                break;
            }
            self.pos += 1;
        }
        self.text
            .get(start..self.pos)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("expected a name at byte {start}"))
    }

    fn skip_whitespace(&mut self) {
        while let Some(&byte) = self.text.as_bytes().get(self.pos) {
            if !byte.is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
    }

    /// Skip an element's content wholesale, up to its matching closing tag. Text inside
    /// is never tokenized: captured output may contain something that looks like markup.
    fn skip_element(&mut self, name: &str) -> Result<(), String> {
        let needle = format!("</{name}");
        loop {
            let rest = self.rest()?;
            let Some(offset) = rest.find(&needle) else {
                return Err(format!("unclosed `<{name}>`"));
            };
            let at = self.pos + offset;
            let after = self
                .text
                .get(at + needle.len()..)
                .ok_or_else(|| format!("unclosed `<{name}>`"))?;
            match after.as_bytes().first() {
                Some(b'>') => {
                    self.pos = at + needle.len() + 1;
                    return Ok(());
                }
                Some(byte) if byte.is_ascii_whitespace() => {
                    let Some(gt) = after.find('>') else {
                        return Err(format!("unclosed `<{name}>`"));
                    };
                    self.pos = at + needle.len() + gt + 1;
                    return Ok(());
                }
                _ => {
                    // `</namex>`: a longer name, not this element's closing tag.
                    self.pos = at + needle.len();
                }
            }
        }
    }
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'-' | b'.')
}

/// Decode XML entities in an attribute value. Unknown entities are an error, never text:
/// a reader that kept `&bogus;` literal would silently invent an id.
fn decode_entities(raw: &str) -> Result<String, String> {
    let mut decoded = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(offset) = rest.find('&') {
        let Some(before) = rest.get(..offset) else {
            return Err("entity decoding lost a character boundary".to_string());
        };
        decoded.push_str(before);
        let Some(entity) = rest.get(offset..) else {
            return Err("entity decoding lost a character boundary".to_string());
        };
        let Some(semicolon) = entity.find(';') else {
            return Err(format!("unterminated XML entity in `{raw}`"));
        };
        let Some(name) = entity.get(1..semicolon) else {
            return Err("entity decoding lost a character boundary".to_string());
        };
        decoded.push(decode_entity(name)?);
        let Some(tail) = entity.get(semicolon + 1..) else {
            return Err("entity decoding lost a character boundary".to_string());
        };
        rest = tail;
    }
    decoded.push_str(rest);
    Ok(decoded)
}

fn decode_entity(name: &str) -> Result<char, String> {
    match name {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        _ => {
            let numeric = name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .map(|hex| (hex, 16))
                .or_else(|| name.strip_prefix('#').map(|decimal| (decimal, 10)));
            let Some((digits, radix)) = numeric else {
                return Err(format!("unknown XML entity `&{name};`"));
            };
            let code = u32::from_str_radix(digits, radix)
                .map_err(|_| format!("malformed numeric XML entity `&{name};`"))?;
            char::from_u32(code)
                .ok_or_else(|| format!("invalid code point in XML entity `&{name};`"))
        }
    }
}

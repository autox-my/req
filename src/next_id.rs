//! `req next-id`: the next free claim ids, so claims authored in parallel do not collide (issue #21).
//!
//! Claim ids are labels a human reads (`REQ-0143`), and "take the next free one" used to be a human
//! step that two authors in two worktrees both performed. This command makes it a computation:
//!
//! * **Highest existing number + 1.** A gap left by a retired or never-used number is not refilled:
//!   a number once seen in a review, a commit message or a Baseline must not come to mean another
//!   claim. (The workflow rule is "never renumber".)
//! * **File names, not parses.** `claims/<id>.yml` *is* the id (`docs/claim-format.md`), so the
//!   answer is available while a sibling claim is half-written or fails a pin. A name that is not
//!   `PREFIX-DIGITS` with a `.yml`/`.yaml` extension holds no number.
//! * **Reservations.** `.req/reserved` lists blocks taken but not yet written (`REQ-0010..REQ-0019`,
//!   or one id; `#` starts a comment). `--reserve` appends the block it hands out. Two branches that
//!   reserve at once both edit the end of that one file, so git reports the collision as a merge
//!   conflict: the file is the signal, not a lock (no locking across machines is claimed).
//! * **Never guesses.** A reservation file that cannot be read or understood is UNVERIFIABLE (exit
//!   3) and prints no id; two prefixes in one corpus with no `--prefix` is a usage error.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;

const USAGE: &str = "usage: req next-id [--count N] [--prefix PREFIX] [--reserve]";
const RESERVED: &str = ".req/reserved";
/// More than a person asks for at once; also bounds the output of a typo.
const MAX_COUNT: u64 = 10_000;
const MIN_WIDTH: usize = 4;
const DEFAULT_PREFIX: &str = "REQ";

fn usage_error(message: &str) -> i32 {
    eprintln!("{message}\n{USAGE}");
    2
}

/// `PREFIX` as the corpus spells it: upper-case letters and digits, starting with a letter.
fn valid_prefix(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// `(prefix, number, digits)` of `PREFIX-DIGITS`, or `None` when it is not an id.
fn parse_id(text: &str) -> Option<(&str, u64, usize)> {
    let (prefix, digits) = text.rsplit_once('-')?;
    if !valid_prefix(prefix) || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((prefix, digits.parse().ok()?, digits.len()))
}

/// What the corpus and the reservations hold for one prefix.
#[derive(Default)]
struct Taken {
    highest: u64,
    width: usize,
}

impl Taken {
    fn see(&mut self, number: u64, digits: usize) {
        self.highest = self.highest.max(number);
        self.width = self.width.max(digits);
    }
}

/// A block from `.req/reserved`: `REQ-0010..REQ-0019` or a single `REQ-0010`.
fn parse_block(line: &str) -> Result<(String, u64, u64, usize), String> {
    let one = |text: &str| {
        parse_id(text)
            .map(|(p, n, d)| (p.to_string(), n, d))
            .ok_or_else(|| format!("`{text}` is not an id like REQ-0010"))
    };
    match line.split_once("..") {
        None => {
            let (prefix, n, digits) = one(line)?;
            Ok((prefix, n, n, digits))
        }
        Some((a, b)) => {
            let (prefix, first, digits) = one(a)?;
            let (other, last, digits_last) = one(b)?;
            if prefix != other {
                return Err(format!(
                    "`{line}` spans two prefixes ({prefix} and {other})"
                ));
            }
            if last < first {
                return Err(format!("`{line}` ends before it starts"));
            }
            Ok((prefix, first, last, digits.max(digits_last)))
        }
    }
}

/// Every reservation, or the reason the file cannot be trusted.
fn read_reservations(corpus: &Path) -> Result<Vec<(String, u64, u64, usize)>, String> {
    let path = corpus.join(RESERVED);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{RESERVED} cannot be read: {err}")),
    };
    let mut blocks = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.chars().any(char::is_whitespace) {
            return Err(format!(
                "{RESERVED} line {}: `{line}` is not one id or one range",
                index + 1
            ));
        }
        blocks.push(
            parse_block(line).map_err(|why| format!("{RESERVED} line {}: {why}", index + 1))?,
        );
    }
    Ok(blocks)
}

pub fn run(args: &[String], corpus: &Path) -> i32 {
    let (mut count, mut prefix, mut reserve) = (None::<u64>, None::<String>, false);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--count" => {
                let Some(value) = iter.next() else {
                    return usage_error("--count needs a number");
                };
                if count.is_some() {
                    return usage_error("--count was given twice");
                }
                match value.parse::<u64>() {
                    Ok(n)
                        if (1..=MAX_COUNT).contains(&n)
                            && value.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        count = Some(n);
                    }
                    _ => {
                        return usage_error(&format!(
                            "--count must be a whole number from 1 to {MAX_COUNT}, not `{value}`"
                        ));
                    }
                }
            }
            "--prefix" => {
                let Some(value) = iter.next() else {
                    return usage_error("--prefix needs a prefix such as REQ");
                };
                if prefix.is_some() {
                    return usage_error("--prefix was given twice");
                }
                if !valid_prefix(value) {
                    return usage_error(&format!(
                        "--prefix must be upper-case letters and digits starting with a letter, not `{value}`"
                    ));
                }
                prefix = Some(value.clone());
            }
            "--reserve" => reserve = true,
            other if other.starts_with('-') => {
                return usage_error(&format!("unknown flag: {other}"));
            }
            other => return usage_error(&format!("unexpected argument: {other}")),
        }
    }
    let count = count.unwrap_or(1);

    // The claim files: a name is an id, and only a file with a claim extension counts.
    let dir = corpus.join("claims");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("[UNVERIFIABLE] {} cannot be read: {err}", dir.display());
            return 3;
        }
    };
    let mut claims: Vec<(String, u64, usize)> = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(stem) = name
            .strip_suffix(".yml")
            .or_else(|| name.strip_suffix(".yaml"))
        else {
            continue;
        };
        if let Some((p, n, d)) = parse_id(stem) {
            claims.push((p.to_string(), n, d));
        }
    }

    let reservations = match read_reservations(corpus) {
        Ok(blocks) => blocks,
        Err(why) => {
            eprintln!("[UNVERIFIABLE] {why}");
            return 3;
        }
    };

    // Which prefix: the one asked for; else the corpus's only one; else REQ if the corpus uses it;
    // else a choice the caller must make.
    let prefix = match prefix {
        Some(p) => p,
        None => {
            let seen: BTreeSet<&str> = claims.iter().map(|(p, _, _)| p.as_str()).collect();
            match seen.len() {
                0 => DEFAULT_PREFIX.to_string(),
                1 => seen
                    .iter()
                    .next()
                    .map(|p| (*p).to_string())
                    .unwrap_or_default(),
                _ if seen.contains(DEFAULT_PREFIX) => DEFAULT_PREFIX.to_string(),
                _ => {
                    return usage_error(&format!(
                        "the corpus uses several id prefixes ({}); choose one with --prefix",
                        seen.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
    };

    let mut taken = Taken {
        highest: 0,
        width: MIN_WIDTH,
    };
    for (p, n, d) in &claims {
        if *p == prefix {
            taken.see(*n, *d);
        }
    }
    for (p, _first, last, d) in &reservations {
        if *p == prefix {
            taken.see(*last, *d);
        }
    }

    let Some(first) = taken.highest.checked_add(1) else {
        eprintln!("[UNVERIFIABLE] the numbers of {prefix} are exhausted");
        return 3;
    };
    let Some(last) = first.checked_add(count - 1) else {
        eprintln!("[UNVERIFIABLE] the numbers of {prefix} are exhausted");
        return 3;
    };
    let width = taken.width;
    let id = |n: u64| format!("{prefix}-{n:0width$}");

    if reserve {
        let line = if first == last {
            id(first)
        } else {
            format!("{}..{}", id(first), id(last))
        };
        if let Err(why) = append_reservation(corpus, &line) {
            eprintln!("[UNVERIFIABLE] {RESERVED} cannot be written: {why}");
            return 3;
        }
    }
    for n in first..=last {
        println!("{}", id(n));
    }
    0
}

/// Append one line, first ending the previous one when the file does not end in a newline.
fn append_reservation(corpus: &Path, line: &str) -> std::io::Result<()> {
    let path = corpus.join(RESERVED);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read(&path).unwrap_or_default();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    if !existing.is_empty() && existing.last() != Some(&b'\n') {
        file.write_all(b"\n")?;
    }
    file.write_all(format!("{line}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_read_strictly() {
        assert_eq!(parse_id("REQ-0001"), Some(("REQ", 1, 4)));
        assert_eq!(parse_id("AB2-00123"), Some(("AB2", 123, 5)));
        for not in [
            "",
            "REQ",
            "REQ-",
            "-0001",
            "req-0001",
            "REQ-00x1",
            "1REQ-0001",
            "REQ-0001-",
        ] {
            assert_eq!(parse_id(not), None, "{not:?}");
        }
    }

    #[test]
    fn a_block_is_one_id_or_one_ordered_range_of_one_prefix() {
        assert_eq!(parse_block("REQ-0010"), Ok(("REQ".to_string(), 10, 10, 4)));
        assert_eq!(
            parse_block("REQ-0010..REQ-0019"),
            Ok(("REQ".to_string(), 10, 19, 4))
        );
        assert!(parse_block("REQ-0019..REQ-0010").is_err());
        assert!(parse_block("REQ-0010..FOO-0011").is_err());
        assert!(parse_block("REQ-0010..").is_err());
    }
}

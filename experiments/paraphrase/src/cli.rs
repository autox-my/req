//! The command line. Exit codes follow req: 0 PASS, 1 FAIL, 3 UNVERIFIABLE, 2 usage.

use crate::data::{
    GeneratorFile, JudgeFile, Statements, alteration_id, read_json, validate_generator,
    validate_judge, validate_statements,
};
use crate::{protocol, sampler, score};
use std::path::{Path, PathBuf};

const USAGE: &str = "usage:
  req-paraphrase --self-check
  req-paraphrase sample [--corpus <file>] [--repo-root <dir>] [--count <n>] [--out <file>]
  req-paraphrase tasks generate [--dir <dir>]
  req-paraphrase tasks judge --generator <id> [--dir <dir>]
  req-paraphrase validate [--dir <dir>]
  req-paraphrase score [--dir <dir>] [--registry <file>] [--record-dir <dir> | --no-record] [--format text|json]
<dir> is the data root holding statements.json, restatements/, judgements/ and results/
(default: this crate's directory). exit: 0 PASS, 1 FAIL, 3 UNVERIFIABLE, 2 usage.";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    crate_dir().join("../..")
}

struct Args<'a> {
    rest: &'a [String],
}

impl<'a> Args<'a> {
    fn value(&self, flag: &str) -> Result<Option<&'a str>, String> {
        match self.rest.iter().position(|a| a == flag) {
            None => Ok(None),
            Some(i) => match self.rest.get(i + 1) {
                Some(v) if !v.starts_with("--") => Ok(Some(v.as_str())),
                _ => Err(format!("{flag} needs a value")),
            },
        }
    }
    fn has(&self, flag: &str) -> bool {
        self.rest.iter().any(|a| a == flag)
    }
    /// Every `--flag` must be one the command knows; a stray one is a usage error.
    fn only(&self, known: &[&str]) -> Result<(), String> {
        let mut skip = false;
        for a in self.rest {
            if skip {
                skip = false;
                continue;
            }
            if !known.contains(&a.as_str()) {
                return Err(format!("unexpected argument {a:?}"));
            }
            // value-taking flags are the ones not in the boolean set
            if !matches!(a.as_str(), "--no-record") {
                skip = true;
            }
        }
        Ok(())
    }
}

fn usage(msg: &str) -> i32 {
    eprintln!("{msg}\n{USAGE}");
    2
}

pub fn run(args: &[String]) -> i32 {
    let Some(cmd) = args.first() else {
        return usage("no command");
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "--self-check" => {
            if crate::selfcheck::run() == 0 {
                0
            } else {
                1
            }
        }
        "-h" | "--help" => {
            println!("{USAGE}");
            0
        }
        "sample" => sample(rest),
        "tasks" => tasks(rest),
        "validate" => validate(rest),
        "score" => score_cmd(rest),
        other => usage(&format!("unknown command {other:?}")),
    }
}

fn data_dir(a: &Args) -> Result<PathBuf, String> {
    Ok(a.value("--dir")?
        .map(PathBuf::from)
        .unwrap_or_else(crate_dir))
}

fn sample(rest: &[String]) -> i32 {
    let a = Args { rest };
    let go = || -> Result<i32, String> {
        a.only(&["--corpus", "--repo-root", "--count", "--out"])?;
        let root = a
            .value("--repo-root")?
            .map(PathBuf::from)
            .unwrap_or_else(repo_root);
        let corpus = a
            .value("--corpus")?
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("bench/corpus/external-v3.yml"));
        let count: usize = a
            .value("--count")?
            .unwrap_or("60")
            .parse()
            .map_err(|_| "--count must be a number".to_string())?;
        let out = a
            .value("--out")?
            .map(PathBuf::from)
            .unwrap_or_else(|| crate_dir().join("statements.json"));
        match sampler::build(&corpus, &root, count) {
            Ok(s) => {
                let mut text = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
                text.push('\n');
                if let Err(e) = std::fs::write(&out, text) {
                    eprintln!("UNVERIFIABLE: cannot write {}: {e}", out.display());
                    return Ok(3);
                }
                println!(
                    "sampled {} of {} requested statements ({} candidate pairs, {} eligible texts, {} excluded as found quoted in tuning files) -> {}",
                    s.sampling.drawn,
                    s.sampling.requested,
                    s.sampling.candidates,
                    s.sampling.eligible_texts,
                    s.sampling.excluded_tuned,
                    out.display()
                );
                if s.sampling.drawn < s.sampling.requested {
                    eprintln!("UNVERIFIABLE: the corpus yielded fewer statements than requested");
                    return Ok(3);
                }
                Ok(0)
            }
            Err(e) => {
                eprintln!("UNVERIFIABLE: {e}");
                Ok(3)
            }
        }
    };
    go().unwrap_or_else(|e| usage(&e))
}

fn load_statements(dir: &Path) -> Result<(Statements, String), String> {
    let (s, sha): (Statements, String) = read_json(&dir.join("statements.json"))?;
    let errs = validate_statements(&s);
    if errs.is_empty() {
        Ok((s, sha))
    } else {
        Err(format!("statements.json invalid:\n  {}", errs.join("\n  ")))
    }
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn tasks(rest: &[String]) -> i32 {
    let Some(kind) = rest.first() else {
        return usage("tasks needs `generate` or `judge`");
    };
    let a = Args { rest: &rest[1..] };
    let go = || -> Result<i32, String> {
        let dir = data_dir(&a)?;
        match kind.as_str() {
            "generate" => {
                a.only(&["--dir"])?;
                let (s, sha) = match load_statements(&dir) {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("UNVERIFIABLE: {e}");
                        return Ok(3);
                    }
                };
                eprintln!("statements_sha256 {sha}");
                for st in &s.statements {
                    let line = serde_json::json!({"statement_id": st.id, "prompt": protocol::generator_prompt(&st.text)});
                    println!("{line}");
                }
                Ok(0)
            }
            "judge" => {
                a.only(&["--dir", "--generator"])?;
                let gid = a
                    .value("--generator")?
                    .ok_or("tasks judge needs --generator <id>")?;
                let (s, sha) = match load_statements(&dir) {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("UNVERIFIABLE: {e}");
                        return Ok(3);
                    }
                };
                let path = dir.join("restatements").join(format!("{gid}.json"));
                let (g, _): (GeneratorFile, String) = match read_json(&path) {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("UNVERIFIABLE: {e}");
                        return Ok(3);
                    }
                };
                let (errs, _) = validate_generator(&g, &s, &sha, gid);
                if !errs.is_empty() {
                    eprintln!("UNVERIFIABLE: {gid} invalid:\n  {}", errs.join("\n  "));
                    return Ok(3);
                }
                eprintln!("statements_sha256 {sha}");
                for item in &g.items {
                    let st = s
                        .statements
                        .iter()
                        .find(|x| x.id == item.statement_id)
                        .expect("validated");
                    for r in &item.restatements {
                        let line = serde_json::json!({"generator": gid, "item_id": r.id, "prompt": protocol::judge_prompt(&st.text, &r.text)});
                        println!("{line}");
                    }
                    let line = serde_json::json!({"generator": gid, "item_id": alteration_id(&item.statement_id), "prompt": protocol::judge_prompt(&st.text, &item.alteration.text)});
                    println!("{line}");
                }
                Ok(0)
            }
            other => Err(format!("unknown tasks kind {other:?}")),
        }
    };
    go().unwrap_or_else(|e| usage(&e))
}

/// Loads and validates everything under `dir`. Err is the full list of problems.
pub struct Loaded {
    pub statements: Statements,
    pub statements_sha: String,
    pub generators: Vec<(GeneratorFile, String)>,
    pub judges: Vec<JudgeFile>,
    pub notes: Vec<String>,
}

pub fn load_all(dir: &Path) -> Result<Loaded, Vec<String>> {
    let (statements, statements_sha) = load_statements(dir).map_err(|e| vec![e])?;
    let mut errs = Vec::new();
    let mut notes = Vec::new();
    let mut generators = Vec::new();
    for p in json_files(&dir.join("restatements")) {
        match read_json::<GeneratorFile>(&p) {
            Err(e) => errs.push(e),
            Ok((g, sha)) => {
                let (e, uncovered) =
                    validate_generator(&g, &statements, &statements_sha, &stem(&p));
                errs.extend(e.into_iter().map(|m| format!("{}: {m}", p.display())));
                if uncovered > 0 {
                    notes.push(format!(
                        "{}: {uncovered} statement(s) not covered",
                        g.generator.id
                    ));
                }
                generators.push((g, sha));
            }
        }
    }
    let gens_only: Vec<GeneratorFile> = generators.iter().map(|(g, _)| g.clone()).collect();
    let mut judges = Vec::new();
    for p in json_files(&dir.join("judgements")) {
        match read_json::<JudgeFile>(&p) {
            Err(e) => errs.push(e),
            Ok((j, _)) => {
                errs.extend(
                    validate_judge(&j, &statements_sha, &stem(&p), &gens_only)
                        .into_iter()
                        .map(|m| format!("{}: {m}", p.display())),
                );
                judges.push(j);
            }
        }
    }
    if errs.is_empty() {
        Ok(Loaded {
            statements,
            statements_sha,
            generators,
            judges,
            notes,
        })
    } else {
        Err(errs)
    }
}

fn validate(rest: &[String]) -> i32 {
    let a = Args { rest };
    let go = || -> Result<i32, String> {
        a.only(&["--dir"])?;
        let dir = data_dir(&a)?;
        match load_all(&dir) {
            Ok(l) => {
                println!(
                    "valid: {} statements, {} generator file(s), {} judge file(s)",
                    l.statements.statements.len(),
                    l.generators.len(),
                    l.judges.len()
                );
                for n in l.notes {
                    println!("note: {n}");
                }
                Ok(0)
            }
            Err(errs) => {
                eprintln!(
                    "INVALID ({} problem(s)):\n  {}",
                    errs.len(),
                    errs.join("\n  ")
                );
                Ok(1)
            }
        }
    };
    go().unwrap_or_else(|e| usage(&e))
}

fn score_cmd(rest: &[String]) -> i32 {
    let a = Args { rest };
    let go = || -> Result<i32, String> {
        a.only(&[
            "--dir",
            "--registry",
            "--record-dir",
            "--no-record",
            "--format",
        ])?;
        if a.has("--no-record") && a.value("--record-dir")?.is_some() {
            return Err("--no-record beside --record-dir".to_string());
        }
        let dir = data_dir(&a)?;
        let format = a.value("--format")?.unwrap_or("text");
        if !matches!(format, "text" | "json") {
            return Err(format!("--format {format:?}: expected text|json"));
        }
        let reg_path = a
            .value("--registry")?
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root().join("aliases/subjects.yml"));
        // A registry that will not load is UNVERIFIABLE, never an empty one: an empty registry
        // would change the addresses being compared.
        let registry = match score::load_registry(&reg_path.to_string_lossy()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("UNVERIFIABLE: registry: {e}");
                return Ok(3);
            }
        };
        let l = match load_all(&dir) {
            Ok(l) => l,
            Err(errs) => {
                eprintln!(
                    "UNVERIFIABLE: inputs are invalid, nothing scored:\n  {}",
                    errs.join("\n  ")
                );
                return Ok(3);
            }
        };
        let rec = score::score(&score::Inputs {
            statements: &l.statements,
            statements_sha: &l.statements_sha,
            generators: &l.generators,
            judges: &l.judges,
            registry: &registry,
        });
        let (bytes, stem) = score::record_bytes(&rec);
        if format == "json" {
            print!("{}", String::from_utf8_lossy(&bytes));
        } else {
            print!("{}", score::render_text(&rec));
            for n in &l.notes {
                println!("note: {n}");
            }
        }
        if !a.has("--no-record") {
            let rdir = a
                .value("--record-dir")?
                .map(PathBuf::from)
                .unwrap_or_else(|| dir.join("results"));
            let path = rdir.join(format!("{stem}.json"));
            if path.exists() {
                eprintln!(
                    "record already exists for this result: {} — not rewritten",
                    path.display()
                );
            } else if let Err(e) =
                std::fs::create_dir_all(&rdir).and_then(|_| std::fs::write(&path, &bytes))
            {
                eprintln!("UNVERIFIABLE: cannot write {}: {e}", path.display());
                return Ok(3);
            } else {
                eprintln!("recorded {}", path.display());
            }
        }
        Ok(rec.verdict.exit_code())
    };
    go().unwrap_or_else(|e| usage(&e))
}

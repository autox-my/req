//! `req propose <a.yml> <b.yml> [--provider-cmd <cmd>]`: the one Tier-2 seam the core keeps (ADR
//! 0021). A provider's answer about ONE explicit pair is an **unratified proposal**, never an edit:
//! nothing is written, no address moves, and no provider is UNVERIFIABLE (never "unrelated").
use crate::model::ClaimFile;
use std::{collections::BTreeSet, io::Write, process::Stdio};

fn load(path: &str) -> Result<ClaimFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    crate::canon::parse_claim(&text).map_err(|e| format!("{path}: {e}"))
}

fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect()
}

/// The funnel's line protocol to the command; the label (with a stated confidence) back.
fn ask(cmd: &str, a: &ClaimFile, b: &ClaimFile) -> Result<String, String> {
    let (wa, wb) = (words(&a.statement), words(&b.statement));
    let diff: Vec<_> = wa.symmetric_difference(&wb).cloned().collect();
    let (id, sa, sb) = (&a.claim_id, a.statement.trim(), b.statement.trim());
    let request = format!(
        "ID: {id}\nA: {sa}\nB: {sb}\nDIFF: {}\nCHOOSE: equivalent|related|unrelated\n",
        diff.join("|")
    );
    let mut sh = std::process::Command::new("sh");
    sh.args(["-c", cmd])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = sh
        .spawn()
        .map_err(|e| format!("cannot run the provider: {e}"))?;
    // A provider that exits unread breaks the pipe; its exit status says why, not this write.
    let _ = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(request.as_bytes()));
    let out = child
        .wait_with_output()
        .map_err(|e| format!("the provider failed: {e}"))?;
    if !out.status.success() {
        return Err(format!("the provider command exited {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let said = json["choice"].as_str().unwrap_or(&text);
    // `unrelated` contains `related`, so it is tested first.
    let label = ["unrelated", "related", "equivalent"]
        .into_iter()
        .find(|l| said.contains(l));
    let label = label.ok_or_else(|| format!("no label in the provider's answer: {text}"))?;
    let stated = json["confidence"]
        .as_u64()
        .map(|c| format!(" (confidence {}%)", c.min(100)));
    Ok(format!("{label}{}", stated.unwrap_or_default()))
}

/// Exit: **0** a proposal was produced, **3** no provider, or it declined or failed, **2** usage.
pub fn run(args: &[String], _root: &std::path::Path) -> i32 {
    let (mut paths, mut cmd, mut rest) = (Vec::new(), None, args.iter());
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--provider-cmd" => cmd = rest.next(),
            flag if flag.starts_with("--") => return usage(),
            _ => paths.push(arg),
        }
    }
    let [a, b] = paths[..] else { return usage() };
    let Some(cmd) = cmd else {
        return unverifiable("no Tier-2 provider is configured (--provider-cmd)");
    };
    match load(a).and_then(|a| ask(cmd, &a, &load(b)?)) {
        Ok(label) => {
            println!("proposal (unratified): {label}\n  provider: cmd:{cmd}");
            println!("  nothing was written; a human ratifies");
            0
        }
        Err(why) => unverifiable(&why),
    }
}

fn usage() -> i32 {
    eprintln!("usage: req propose <a.yml> <b.yml> [--provider-cmd <cmd>]");
    2
}

fn unverifiable(why: &str) -> i32 {
    eprintln!("[UNVERIFIABLE] {why}");
    3
}

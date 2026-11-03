//! `req-tier2` — the funnel's command line: `req-tier2 [--root <dir>] [--min-shared N]
//! [--max-pairs N] [--gold <file>] [--provider-cmd <cmd>] [--repeats N] [--min-confidence N]
//! [--pairs] [key=value ...]`. It is the former `req propose`, moved out of the core (ADR 0021);
//! the corpus root is resolved the way the core resolves it (ADR 0012).
use req::root;
use req_tier2::funnel;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (cli_root, rest) = match root::take_root_arg(&argv) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let env_req_root = std::env::var("REQ_ROOT").ok();
    let corpus = match root::resolve_corpus_root(
        std::env::current_dir,
        cli_root.as_deref(),
        env_req_root.as_deref(),
    ) {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("[UNVERIFIABLE] {message}");
            std::process::exit(3);
        }
    };
    eprintln!("corpus root: {}", corpus.display());
    std::process::exit(funnel::run(&rest, &corpus));
}

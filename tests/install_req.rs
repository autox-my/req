//! `examples/install-req.sh` — the install step an adopter copies (ADR 0025, unit 65-2).
//!
//! The seam is the script run **as a process**, against a local HTTP server this file starts on
//! `127.0.0.1` and a fixture tarball it builds itself. No release exists yet and none of these
//! tests may reach the real host, so the two test-only overrides the script documents
//! (`REQ_RELEASE_BASE`, `REQ_CURL_PROTO`) are set **here and nowhere else** — no shipped YAML
//! carries them (`tests/ci_template.rs` pins that).
//!
//! The oracles are independent of the script: the pinned hash is computed here with `sha2` (the
//! script uses `sha256sum`/`shasum`), the archives are written byte by byte by a ten-line `ustar`
//! writer (so a `..` or `/abs` member name exists exactly as written, which no `tar` front end
//! would produce), and every expected message is hand-written. The script's one job is to fail
//! **closed**: every refusal below must leave nothing installed and say why.
//!
//! The release is public (ADR 0029 §5): the default host needs no credential, so the happy path runs
//! against a host that asks for none and **no `Authorization` header may be sent**; the header
//! exists only when `REQ_RELEASE_TOKEN` is set and non-empty (a private fork), and that case runs
//! against a host that refuses everything else.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SCRIPT: &str = "examples/install-req.sh";

/// The rev the fixtures pin: a bare short SHA, the shape `built from` prints for a clean build.
const REV: &str = "abc1234";
/// The target the `uname` shim reports, so the test is the same on a Linux runner and on macOS.
const TARGET: &str = "x86_64-unknown-linux-gnu";
const TOKEN: &str = "tok_SECRET_9f3a1c77d2";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

// ---------------------------------------------------------------------------
// An independent oracle: the hash, the archive, the server
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One `ustar` member: `(name, typeflag, mode, content)`. Written by hand so that `../req`,
/// `/abs/req` and `bin/req` are in the archive exactly as spelled.
fn member(name: &str, content: &[u8]) -> Vec<u8> {
    member_typed(name, b'0', content)
}

fn member_typed(name: &str, typeflag: u8, content: &[u8]) -> Vec<u8> {
    let mut header = [0u8; 512];
    let put = |header: &mut [u8; 512], at: usize, text: &str| {
        header[at..at + text.len()].copy_from_slice(text.as_bytes());
    };
    put(&mut header, 0, name);
    put(&mut header, 100, "0000755\0");
    put(&mut header, 108, "0000000\0");
    put(&mut header, 116, "0000000\0");
    let size = if typeflag == b'0' { content.len() } else { 0 };
    put(&mut header, 124, &format!("{size:011o}\0"));
    put(&mut header, 136, "00000000000\0");
    put(&mut header, 148, "        ");
    header[156] = typeflag;
    if typeflag == b'2' {
        // A symlink member: the link target rides in `linkname`.
        put(&mut header, 157, &String::from_utf8_lossy(content));
    }
    put(&mut header, 257, "ustar\0");
    put(&mut header, 263, "00");
    let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
    put(&mut header, 148, &format!("{sum:06o}\0 "));
    let mut out = header.to_vec();
    if typeflag == b'0' {
        out.extend_from_slice(content);
        let pad = (512 - content.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out
}

/// `members` as a `.tar.gz` (the compression is the system `gzip`, an independent tool).
fn tar_gz(members: &[Vec<u8>]) -> Vec<u8> {
    let mut tar: Vec<u8> = members.concat();
    tar.extend(std::iter::repeat_n(0u8, 1024));
    let mut child = Command::new("gzip")
        .arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawns gzip");
    let mut stdin = child.stdin.take().expect("gzip stdin");
    let writer = std::thread::spawn(move || {
        stdin.write_all(&tar).expect("writes the tar to gzip");
    });
    let out = child.wait_with_output().expect("gzip finishes");
    writer.join().expect("the writer thread ends");
    assert!(out.status.success(), "gzip failed");
    out.stdout
}

/// What `req --version` prints, in `src/main.rs`'s layout (label column 19 wide).
fn version_text(built_from: &str) -> String {
    let mut text = String::from("req 0.1.0\n");
    for (label, value) in [
        ("rules_v", "6"),
        ("baseline_envelope", "1"),
        ("decision_chain", "1"),
        ("claim_schema", "req.claim/2"),
        ("built from", built_from),
    ] {
        text.push_str(&format!("  {label:<19}{value}\n"));
    }
    text
}

/// A stub `req`: a shell script that answers `--version` and nothing else.
fn stub_req(built_from: &str) -> Vec<u8> {
    format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\ncat <<'EOF'\n{}EOF\nexit 0\nfi\nexit 2\n",
        version_text(built_from)
    )
    .into_bytes()
}

/// The tarball `build-release.sh` would produce: exactly one member, `req`.
fn good_tarball(built_from: &str) -> Vec<u8> {
    tar_gz(&[member("req", &stub_req(built_from))])
}

#[derive(Clone, Debug)]
struct Seen {
    path: String,
    authorization: Option<String>,
}

struct Reply {
    status: u16,
    body: Vec<u8>,
}

type Handler = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

/// A throwaway HTTP/1.1 server on `127.0.0.1:<free port>`, one thread, `Connection: close`.
struct Server {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start(handler: Handler) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds a loopback port");
        listener
            .set_nonblocking(true)
            .expect("makes the listener non-blocking");
        let port = listener.local_addr().expect("local addr").port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen_in, stop_in) = (Arc::clone(&seen), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            while !stop_in.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        serve(stream, &handler, &seen_in);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Server {
            port,
            seen,
            stop,
            thread: Some(thread),
        }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn requests(&self) -> Vec<Seen> {
        self.seen.lock().expect("the seen log").clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, handler: &Handler, seen: &Mutex<Vec<Seen>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.split("\r\n");
    let path = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or_default()
        .to_string();
    let authorization = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim().to_string());
    let request = Seen {
        path,
        authorization,
    };
    let reply = handler(&request);
    seen.lock().expect("the seen log").push(request);
    let reason = match reply.status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Other",
    };
    let head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&reply.body);
}

/// A server that answers `Authorization: token <TOKEN>` with the routed body and anything else
/// with 401 — the private host's behaviour, which is what makes a missing token observable.
fn private_host(routes: Vec<(String, Vec<u8>)>) -> Server {
    let table: HashMap<String, Vec<u8>> = routes.into_iter().collect();
    Server::start(Arc::new(move |request: &Seen| {
        if request.authorization.as_deref() != Some(&format!("token {TOKEN}")) {
            return Reply {
                status: 401,
                body: b"unauthorized".to_vec(),
            };
        }
        match table.get(&request.path) {
            Some(body) => Reply {
                status: 200,
                body: body.clone(),
            },
            None => Reply {
                status: 404,
                body: b"not found".to_vec(),
            },
        }
    }))
}

/// A server that serves the routed body to anyone, as a public release does. It still records the
/// `Authorization` header it was sent, which is what makes "no header without a token" observable.
fn public_host(routes: Vec<(String, Vec<u8>)>) -> Server {
    let table: HashMap<String, Vec<u8>> = routes.into_iter().collect();
    Server::start(Arc::new(move |request: &Seen| {
        match table.get(&request.path) {
            Some(body) => Reply {
                status: 200,
                body: body.clone(),
            },
            None => Reply {
                status: 404,
                body: b"not found".to_vec(),
            },
        }
    }))
}

fn asset_path(rev: &str) -> String {
    format!("/rev-{rev}/req-{rev}-{TARGET}.tar.gz")
}

// ---------------------------------------------------------------------------
// The run: a scratch directory, the script with its pin table filled in, an environment
// ---------------------------------------------------------------------------

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("req-install-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creates the scratch dir");
        Scratch(dir)
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The shipped script with its three pin-table lines replaced — the only edit an adopter makes.
fn script_with_pins(rev: &str, linux: &str, darwin: &str) -> String {
    let source = std::fs::read_to_string(repo_root().join(SCRIPT))
        .unwrap_or_else(|error| panic!("{SCRIPT} must exist and be readable: {error}"));
    let mut left = [
        "REV=",
        "SHA256_x86_64_unknown_linux_gnu=",
        "SHA256_aarch64_apple_darwin=",
    ];
    let values = [rev, linux, darwin];
    let mut out = String::new();
    for line in source.lines() {
        let mut replaced = false;
        for (at, key) in left.iter_mut().enumerate() {
            if !key.is_empty() && line.starts_with(*key) {
                out.push_str(&format!("{key}{}\n", values[at]));
                *key = "";
                replaced = true;
                break;
            }
        }
        if !replaced {
            out.push_str(line);
            out.push('\n');
        }
    }
    assert!(
        left.iter().all(|key| key.is_empty()),
        "{SCRIPT} must open with the pin table: REV=, SHA256_x86_64_unknown_linux_gnu=, \
         SHA256_aarch64_apple_darwin= (one line each)"
    );
    out
}

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn text(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// How one run is set up. `Default` is the happy path's environment.
struct Run<'a> {
    scratch: &'a Scratch,
    /// The script text to run (pins already filled in).
    script: String,
    /// The `REQ_RELEASE_BASE`, or none.
    base: Option<String>,
    token: Option<&'a str>,
    /// The `REQ_CURL_PROTO`, or none (the script's own default, `=https`).
    proto: Option<&'a str>,
    /// The `uname -sm` the shim prints.
    uname: &'a str,
    install_dir: Option<PathBuf>,
    /// When set, `PATH` holds the shim and symlinks to exactly these tools — nothing else — so a
    /// tool the script must cope without (`sha256sum`) is genuinely absent.
    only_tools: Option<Vec<&'a str>>,
}

impl<'a> Run<'a> {
    fn new(scratch: &'a Scratch, server: &Server, linux_pin: &str) -> Run<'a> {
        Run {
            scratch,
            script: script_with_pins(REV, linux_pin, "0".repeat(64).as_str()),
            base: Some(server.base()),
            token: None,
            proto: Some("=http"),
            uname: "Linux x86_64",
            install_dir: None,
            only_tools: None,
        }
    }

    /// The directory `req` is installed into (the default, `$HOME/.local/bin`, unless overridden).
    fn install_dir(&self) -> PathBuf {
        self.install_dir
            .clone()
            .unwrap_or_else(|| self.scratch.path("home/.local/bin"))
    }

    fn go(&self) -> Outcome {
        let script_path = self.scratch.path("install-req.sh");
        std::fs::write(&script_path, &self.script).expect("writes the script");
        // The `uname` shim: first on PATH, so the platform is the test's to choose.
        let shim_dir = self.scratch.path("shim");
        std::fs::create_dir_all(&shim_dir).expect("creates the shim dir");
        let shim = shim_dir.join("uname");
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"-sm\" ]; then echo '{}'; exit 0; fi\nexec /usr/bin/uname \"$@\"\n",
                self.uname
            ),
        )
        .expect("writes the uname shim");
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
            .expect("makes the shim executable");
        std::fs::create_dir_all(self.scratch.path("home")).expect("creates HOME");

        let ambient = std::env::var("PATH").unwrap_or_default();
        let path = match &self.only_tools {
            None => format!("{}:{ambient}", shim_dir.display()),
            Some(tools) => {
                let tool_dir = self.scratch.path("tools");
                std::fs::create_dir_all(&tool_dir).expect("creates the tools dir");
                for tool in tools {
                    let found = ambient
                        .split(':')
                        .map(|dir| Path::new(dir).join(tool))
                        .find(|candidate| candidate.is_file())
                        .unwrap_or_else(|| panic!("`{tool}` is not on this machine's PATH"));
                    std::os::unix::fs::symlink(found, tool_dir.join(tool)).expect("links the tool");
                }
                format!("{}:{}", shim_dir.display(), tool_dir.display())
            }
        };
        // Resolved against the ambient PATH: the restricted one may not hold `bash` at all.
        let bash = ambient
            .split(':')
            .map(|dir| Path::new(dir).join("bash"))
            .find(|candidate| candidate.is_file())
            .expect("bash is on PATH");
        let mut command = Command::new(bash);
        command
            .arg(&script_path)
            .current_dir(self.scratch.path("home"))
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.scratch.path("home"))
            .env("TMPDIR", self.scratch.path("home"));
        if let Some(proto) = self.proto {
            command.env("REQ_CURL_PROTO", proto);
        }
        if let Some(base) = &self.base {
            command.env("REQ_RELEASE_BASE", base);
        }
        if let Some(token) = self.token {
            command.env("REQ_RELEASE_TOKEN", token);
        }
        if let Some(dir) = &self.install_dir {
            command.env("REQ_INSTALL_DIR", dir);
        }
        let output = command.output().expect("runs the install script");
        Outcome {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// Nothing — not `req`, not a staging directory, not a partial download — is left where `req`
/// would have been installed. A refusal that leaves debris is a refusal that half-installed.
fn assert_nothing_installed(dir: &Path, why: &str) {
    assert!(
        !dir.join("req").exists(),
        "{why}: `req` must not be installed at {}",
        dir.display()
    );
    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        leftovers.is_empty(),
        "{why}: the install directory must be left empty, found {leftovers:?}"
    );
}

fn failed(outcome: &Outcome, what: &str) {
    assert!(
        matches!(outcome.code, Some(code) if code != 0),
        "{what}: expected a non-zero exit, got {:?}\n{}",
        outcome.code,
        outcome.text()
    );
}

/// The server holding the good tarball at the pinned rev; returns it and the tarball's hash.
fn serving_good(built_from: &str) -> (Server, String) {
    let tarball = good_tarball(built_from);
    let hash = sha256_hex(&tarball);
    (public_host(vec![(asset_path(REV), tarball)]), hash)
}

/// The same release behind a private host: only `Authorization: token <TOKEN>` is served.
fn serving_good_privately(built_from: &str) -> (Server, String) {
    let tarball = good_tarball(built_from);
    let hash = sha256_hex(&tarball);
    (private_host(vec![(asset_path(REV), tarball)]), hash)
}

// ---------------------------------------------------------------------------
// (a) the happy path
// ---------------------------------------------------------------------------

#[test]
fn a_the_happy_path_installs_req_0755_and_prints_the_lines_that_decide_it() {
    let scratch = Scratch::new("happy");
    let (server, hash) = serving_good(REV);
    let run = Run::new(&scratch, &server, &hash);
    let outcome = run.go();

    assert_eq!(outcome.code, Some(0), "{}", outcome.text());
    let installed = run.install_dir().join("req");
    let mode = std::fs::metadata(&installed)
        .unwrap_or_else(|error| {
            panic!(
                "{} must exist: {error}\n{}",
                installed.display(),
                outcome.text()
            )
        })
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o755, "req is installed 0755");
    // The two `--version` lines that decide the binary, as printed by the binary itself.
    let lines: Vec<&str> = outcome.stdout.lines().map(str::trim).collect();
    assert!(
        lines.contains(&"rules_v            6"),
        "the rules_v line is printed:\n{}",
        outcome.text()
    );
    assert!(
        lines.contains(&format!("built from         {REV}").as_str()),
        "the built-from line is printed:\n{}",
        outcome.text()
    );
    // Only `req` is left behind: no staging directory, no tarball.
    let entries: Vec<String> = std::fs::read_dir(run.install_dir())
        .expect("reads the install dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["req".to_string()], "only req is left behind");

    // The wire: one request, the pinned path, NO Authorization header (a public release asks for no
    // credential and the script has none to send), and the release's own `.sha256` asset never read
    // (ADR 0025 §3).
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        1,
        "one download, nothing else: {requests:?}"
    );
    assert_eq!(requests[0].path, asset_path(REV));
    assert_eq!(
        requests[0].authorization, None,
        "no token is set, so no Authorization header may be sent"
    );
}

#[test]
fn a_a_private_fork_sets_the_token_and_the_header_is_sent() {
    let scratch = Scratch::new("private-fork");
    let (server, hash) = serving_good_privately(REV);
    let mut run = Run::new(&scratch, &server, &hash);
    run.token = Some(TOKEN);
    let outcome = run.go();

    assert_eq!(outcome.code, Some(0), "{}", outcome.text());
    assert!(run.install_dir().join("req").is_file());
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some(format!("token {TOKEN}").as_str()),
        "a set token travels as `Authorization: token <it>`"
    );
}

#[test]
fn a_req_install_dir_is_honoured_and_created() {
    let scratch = Scratch::new("install-dir");
    let (server, hash) = serving_good(REV);
    let mut run = Run::new(&scratch, &server, &hash);
    run.install_dir = Some(scratch.path("elsewhere/bin"));
    let outcome = run.go();
    assert_eq!(outcome.code, Some(0), "{}", outcome.text());
    assert!(scratch.path("elsewhere/bin/req").is_file());
}

// ---------------------------------------------------------------------------
// (b) the pinned hash is the authority
// ---------------------------------------------------------------------------

#[test]
fn b_a_wrong_pin_hash_installs_nothing() {
    let scratch = Scratch::new("wrong-hash");
    let (server, hash) = serving_good(REV);
    // Flip one hex digit of the pin.
    let mut flipped: Vec<char> = hash.chars().collect();
    flipped[0] = if flipped[0] == '0' { '1' } else { '0' };
    let wrong: String = flipped.into_iter().collect();
    let run = Run::new(&scratch, &server, &wrong);
    let outcome = run.go();

    failed(&outcome, "a pin that does not match the download");
    assert!(
        outcome.stderr.contains("SHA-256 mismatch"),
        "the refusal names the cause:\n{}",
        outcome.text()
    );
    assert_nothing_installed(&run.install_dir(), "a hash mismatch");
}

#[test]
fn b_a_tampered_tarball_with_a_matching_sidecar_is_still_refused() {
    // The release's `.sha256` sits beside the tarball it describes: a tampered pair agrees
    // with itself. The install must read the *pin*, never the sidecar — a checksum fetched
    // from the place that could be tampered with proves transfer, never authenticity.
    let scratch = Scratch::new("tampered-pair");
    let pinned = sha256_hex(&good_tarball(REV));
    let tampered = tar_gz(&[member("req", &stub_req(REV)), member("req.extra", b"x")]);
    let tampered_hash = sha256_hex(&tampered);
    let sidecar = format!("{tampered_hash}  req-{REV}-{TARGET}.tar.gz\n").into_bytes();
    let server = public_host(vec![
        (asset_path(REV), tampered),
        (format!("{}.sha256", asset_path(REV)), sidecar),
    ]);
    let run = Run::new(&scratch, &server, &pinned);
    let outcome = run.go();

    failed(&outcome, "a tampered pair against an honest pin");
    assert!(
        outcome.stderr.contains("SHA-256 mismatch"),
        "{}",
        outcome.text()
    );
    assert_nothing_installed(&run.install_dir(), "a tampered pair");
    assert!(
        server
            .requests()
            .iter()
            .all(|request| !request.path.ends_with(".sha256")),
        "the install never fetches the release's checksum asset: {:?}",
        server.requests()
    );
}

// ---------------------------------------------------------------------------
// (c) an unknown rev, (d) a refused token
// ---------------------------------------------------------------------------

#[test]
fn c_an_unknown_rev_is_a_404_and_a_non_zero_exit() {
    let scratch = Scratch::new("unknown-rev");
    let server = public_host(vec![]);
    let run = Run::new(&scratch, &server, &sha256_hex(b"whatever"));
    let outcome = run.go();

    failed(&outcome, "a rev with no release");
    assert!(outcome.stderr.contains("HTTP 404"), "{}", outcome.text());
    assert!(
        outcome.stderr.contains(&format!("rev-{REV}")),
        "the refusal names the release it looked for:\n{}",
        outcome.text()
    );
    assert_nothing_installed(&run.install_dir(), "a 404");
}

#[test]
fn d_a_refused_token_is_a_401_that_names_the_token() {
    let scratch = Scratch::new("unauthorized");
    let (server, hash) = serving_good_privately(REV);
    let mut run = Run::new(&scratch, &server, &hash);
    run.token = Some("a_token_the_host_does_not_accept");
    let outcome = run.go();

    failed(&outcome, "a token the host refuses");
    assert!(outcome.stderr.contains("HTTP 401"), "{}", outcome.text());
    assert!(
        outcome.stderr.contains("REQ_RELEASE_TOKEN"),
        "a 401 names the variable to fix:\n{}",
        outcome.text()
    );
    assert_nothing_installed(&run.install_dir(), "a 401");
}

// ---------------------------------------------------------------------------
// (e) the archive is exactly one member, `req`
// ---------------------------------------------------------------------------

#[test]
fn e_an_archive_that_is_not_exactly_one_req_is_refused_before_anything_is_extracted() {
    let planted: &[u8] = b"planted";
    let cases: Vec<(&str, Vec<Vec<u8>>)> = vec![
        (
            "a directory prefix",
            vec![member("bin/req", &stub_req(REV))],
        ),
        (
            "a second member",
            vec![member("req", &stub_req(REV)), member("planted", planted)],
        ),
        (
            "a `..` member beside req",
            vec![member("req", &stub_req(REV)), member("../planted", planted)],
        ),
        (
            "a `..` member alone",
            vec![member("../req", &stub_req(REV))],
        ),
        (
            "an absolute member",
            vec![member(
                "/tmp/req-install-absolute-planted/req",
                &stub_req(REV),
            )],
        ),
        ("a `./` prefix", vec![member("./req", &stub_req(REV))]),
        (
            "a symlink named req",
            vec![member_typed("req", b'2', b"/bin/sh")],
        ),
        ("no member", vec![]),
    ];
    for (index, (name, members)) in cases.into_iter().enumerate() {
        let scratch = Scratch::new(&format!("archive-{index}"));
        let tarball = tar_gz(&members);
        let hash = sha256_hex(&tarball);
        let server = public_host(vec![(asset_path(REV), tarball)]);
        let run = Run::new(&scratch, &server, &hash);
        let outcome = run.go();

        failed(&outcome, name);
        assert!(
            outcome.stderr.contains("exactly one member"),
            "{name}: the refusal names the rule (a refusal for any other reason would let the \
             member check be deleted unnoticed):\n{}",
            outcome.text()
        );
        assert_nothing_installed(&run.install_dir(), name);
        // Nothing was extracted anywhere near the install: not beside it, not above it.
        assert!(
            !run.install_dir().join("planted").exists()
                && !scratch.path("home/.local/planted").exists()
                && !scratch.path("home/planted").exists()
                && !Path::new("/tmp/req-install-absolute-planted").exists(),
            "{name}: a member escaped"
        );
    }
}

// ---------------------------------------------------------------------------
// (f) the binary must report the pinned rev, exactly
// ---------------------------------------------------------------------------

#[test]
fn f_a_binary_that_reports_another_rev_is_refused() {
    for reported in [
        "def5678",
        "abc1234-dirty",
        "unknown",
        "abc1234-cleanliness-unknown",
        // A longer SHA that merely *starts with* the pinned rev is not the pinned rev.
        "abc12345",
    ] {
        let scratch = Scratch::new(&format!("built-from-{reported}"));
        let (server, hash) = serving_good(reported);
        let run = Run::new(&scratch, &server, &hash);
        let outcome = run.go();

        failed(
            &outcome,
            &format!("`built from {reported}` against a pin of {REV}"),
        );
        assert!(
            outcome.stderr.contains("built from"),
            "{reported}: the refusal names the line that decided it:\n{}",
            outcome.text()
        );
        assert_nothing_installed(&run.install_dir(), reported);
    }
}

#[test]
fn f_a_binary_with_no_built_from_line_is_refused() {
    let scratch = Scratch::new("no-built-from");
    let silent =
        b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'req 0.1.0'; exit 0; fi\nexit 2\n";
    let tarball = tar_gz(&[member("req", silent)]);
    let hash = sha256_hex(&tarball);
    let server = public_host(vec![(asset_path(REV), tarball)]);
    let run = Run::new(&scratch, &server, &hash);
    let outcome = run.go();
    failed(&outcome, "a --version with no `built from` line");
    assert_nothing_installed(&run.install_dir(), "no built-from line");
}

// ---------------------------------------------------------------------------
// (g) an unsupported platform, or a target with no pin
// ---------------------------------------------------------------------------

#[test]
fn g_an_unsupported_platform_is_a_non_zero_exit_before_any_download() {
    for uname in [
        "Linux aarch64",
        "Darwin x86_64",
        "FreeBSD amd64",
        "Linux riscv64",
    ] {
        let scratch = Scratch::new(&format!("platform-{}", uname.replace(' ', "-")));
        let (server, hash) = serving_good(REV);
        let mut run = Run::new(&scratch, &server, &hash);
        run.uname = uname;
        let outcome = run.go();

        failed(&outcome, uname);
        assert!(
            outcome.stderr.contains("unsupported platform") && outcome.stderr.contains(uname),
            "{uname}: the refusal says so and names the platform:\n{}",
            outcome.text()
        );
        assert!(
            server.requests().is_empty(),
            "{uname}: nothing is downloaded for a platform that cannot be installed"
        );
        assert_nothing_installed(&run.install_dir(), uname);
    }
}

#[test]
fn g_a_supported_target_with_an_empty_pin_is_a_non_zero_exit_not_a_skip() {
    let scratch = Scratch::new("empty-pin");
    let (server, _hash) = serving_good(REV);
    let mut run = Run::new(&scratch, &server, "");
    // The Linux pin is empty; the darwin pin is irrelevant on this platform.
    run.script = script_with_pins(REV, "", &"0".repeat(64));
    let outcome = run.go();
    failed(&outcome, "an empty pin for the detected target");
    assert!(
        outcome.stderr.contains("no pin for target") && outcome.stderr.contains(TARGET),
        "{}",
        outcome.text()
    );
    assert!(server.requests().is_empty());
}

// ---------------------------------------------------------------------------
// (h) a binary that cannot run
// ---------------------------------------------------------------------------

#[test]
fn h_a_binary_that_cannot_run_is_refused() {
    let garbage: Vec<u8> = (0u8..=255).cycle().take(300).collect();
    for (name, content) in [
        ("random bytes", garbage),
        ("a script that exits 1", b"#!/bin/sh\nexit 1\n".to_vec()),
        (
            "a --version that exits non-zero after printing the right line",
            format!("#!/bin/sh\ncat <<'EOF'\n{}EOF\nexit 7\n", version_text(REV)).into_bytes(),
        ),
    ] {
        let scratch = Scratch::new(&format!("cannot-run-{}", name.len()));
        let tarball = tar_gz(&[member("req", &content)]);
        let hash = sha256_hex(&tarball);
        let server = public_host(vec![(asset_path(REV), tarball)]);
        let run = Run::new(&scratch, &server, &hash);
        let outcome = run.go();

        failed(&outcome, name);
        assert_nothing_installed(&run.install_dir(), name);
    }
}

// ---------------------------------------------------------------------------
// (i) no token: a public release needs none, and a private one says what to set
// ---------------------------------------------------------------------------

#[test]
fn i_no_token_installs_from_a_public_release_and_sends_no_authorization_header() {
    // Unset and set-but-empty are the same answer: an empty value must not become a bare
    // `Authorization: token ` header.
    for token in [None, Some("")] {
        let scratch = Scratch::new(&format!("no-token-{}", token.is_some()));
        let (server, hash) = serving_good(REV);
        let mut run = Run::new(&scratch, &server, &hash);
        run.token = token;
        let outcome = run.go();

        assert_eq!(outcome.code, Some(0), "{}", outcome.text());
        assert!(run.install_dir().join("req").is_file());
        let requests = server.requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert_eq!(
            requests[0].authorization, None,
            "token {token:?}: no Authorization header at all, not even an empty one"
        );
        assert!(
            !outcome.stderr.contains("REQ_RELEASE_TOKEN is not set"),
            "the retired refusal must be gone:\n{}",
            outcome.text()
        );
    }
}

#[test]
fn i_a_private_host_without_a_token_is_a_401_that_names_the_variable() {
    let scratch = Scratch::new("private-no-token");
    let (server, hash) = serving_good_privately(REV);
    let run = Run::new(&scratch, &server, &hash);
    let outcome = run.go();

    failed(&outcome, "a private host and no token");
    assert!(outcome.stderr.contains("HTTP 401"), "{}", outcome.text());
    assert!(
        outcome.stderr.contains("REQ_RELEASE_TOKEN"),
        "the refusal names the variable a private fork sets:\n{}",
        outcome.text()
    );
    assert_nothing_installed(&run.install_dir(), "a 401 with no token");
}

// ---------------------------------------------------------------------------
// (j) the token is never printed
// ---------------------------------------------------------------------------

#[test]
fn j_the_token_never_appears_in_stdout_or_stderr() {
    // The happy path, a 401 (the server is told the wrong token and so is the log), a hash
    // mismatch, a 404 and a build mismatch: every exit path of the script.
    let mut outputs: Vec<(String, Outcome)> = Vec::new();

    let scratch = Scratch::new("secret-happy");
    let (server, hash) = serving_good(REV);
    let mut happy = Run::new(&scratch, &server, &hash);
    happy.token = Some(TOKEN);
    outputs.push(("happy".into(), happy.go()));

    let scratch_mismatch = Scratch::new("secret-mismatch");
    let mut mismatch = Run::new(&scratch_mismatch, &server, &"f".repeat(64));
    mismatch.token = Some(TOKEN);
    outputs.push(("mismatch".into(), mismatch.go()));

    let scratch_404 = Scratch::new("secret-404");
    let empty = private_host(vec![]);
    let mut missing = Run::new(&scratch_404, &empty, &hash);
    missing.token = Some(TOKEN);
    outputs.push(("404".into(), missing.go()));

    let scratch_401 = Scratch::new("secret-401");
    let (private, _) = serving_good_privately(REV);
    let mut refused = Run::new(&scratch_401, &private, &hash);
    refused.token = Some("wrong_but_also_secret_55aa");
    let refused_outcome = refused.go();
    assert!(
        !refused_outcome
            .text()
            .contains("wrong_but_also_secret_55aa"),
        "a refused token is still a secret:\n{}",
        refused_outcome.text()
    );
    outputs.push(("401".into(), refused_outcome));

    for (name, outcome) in &outputs {
        assert!(
            !outcome.text().contains(TOKEN),
            "{name}: the token appeared in the script's output:\n{}",
            outcome.text()
        );
        assert!(
            !outcome.text().contains("Authorization"),
            "{name}: the header line appeared in the script's output:\n{}",
            outcome.text()
        );
    }
}

#[test]
fn j_the_script_source_keeps_the_token_off_the_command_line_and_the_trace_off() {
    let source = std::fs::read_to_string(repo_root().join(SCRIPT)).expect("reads the script");
    for line in source.lines() {
        let code = line.trim_start();
        if code.starts_with('#') {
            continue;
        }
        assert!(
            !code.contains("set -x") && !code.contains("xtrace") && !code.contains("set -v"),
            "no trace, ever: it would print the header the token rides in:\n{line}"
        );
        assert!(
            !(code.contains("-H \"Authorization") || code.contains("-H 'Authorization")),
            "the token is sent from a header file or `-H @-`, never as a command-line argument \
             (`ps` shows it):\n{line}"
        );
        assert!(
            !(code.contains("curl") && code.contains("REQ_RELEASE_TOKEN")),
            "the token is not on the `curl` line:\n{line}"
        );
    }
    assert!(
        source.contains("-H @-"),
        "the Authorization header is read from stdin (`-H @-`)"
    );
}

// ---------------------------------------------------------------------------
// The shipped file: pin table first, placeholders fail closed, defaults pinned
// ---------------------------------------------------------------------------

#[test]
fn k_the_shipped_placeholders_fail_closed_and_say_so() {
    let scratch = Scratch::new("placeholders");
    let server = public_host(vec![]);
    let mut run = Run::new(&scratch, &server, "");
    // The script exactly as shipped: nobody has filled the pin table in.
    run.script = std::fs::read_to_string(repo_root().join(SCRIPT)).expect("reads the script");
    let outcome = run.go();
    failed(&outcome, "the unfilled pin table");
    assert!(
        outcome.stderr.contains("pin table"),
        "the refusal points at the pin table:\n{}",
        outcome.text()
    );
    assert!(
        server.requests().is_empty(),
        "nothing is downloaded on a placeholder"
    );
    assert_nothing_installed(&run.install_dir(), "a placeholder");
}

#[test]
fn k_a_malformed_pin_is_refused_not_downloaded() {
    for (rev, pin, what) in [
        ("not-a-sha", "a".repeat(64), "a REV that is not hex"),
        ("ABC1234", "a".repeat(64), "an upper-case REV"),
        ("abc12", "a".repeat(64), "a REV shorter than 7"),
        (REV, "a".repeat(63), "a 63-digit hash"),
        (REV, "g".repeat(64), "a non-hex hash"),
    ] {
        let scratch = Scratch::new(&format!("malformed-{}", what.len()));
        let (server, _hash) = serving_good(REV);
        let mut run = Run::new(&scratch, &server, &pin);
        run.script = script_with_pins(rev, &pin, &"0".repeat(64));
        let outcome = run.go();
        failed(&outcome, what);
        assert!(
            outcome.stderr.contains("pin table"),
            "{what}:\n{}",
            outcome.text()
        );
        assert!(
            server.requests().is_empty(),
            "{what}: nothing is downloaded"
        );
    }
}

/// Every tool the script needs besides the SHA-256 one and the `uname` shim (and `cat`, which the
/// fixture's stub `req` uses to print its version).
const BASE_TOOLS: [&str; 13] = [
    "curl", "tar", "gzip", "mktemp", "mkdir", "chmod", "mv", "rm", "sed", "cut", "head", "tr",
    "cat",
];

#[test]
fn n_without_sha256sum_the_shasum_branch_verifies_against_the_pin() {
    for (name, pin_is_right) in [("right", true), ("wrong", false)] {
        let scratch = Scratch::new(&format!("shasum-{name}"));
        let (server, hash) = serving_good(REV);
        let pin = if pin_is_right { hash } else { "e".repeat(64) };
        let mut run = Run::new(&scratch, &server, &pin);
        let mut tools = BASE_TOOLS.to_vec();
        tools.extend(["shasum", "perl"]);
        run.only_tools = Some(tools);
        let outcome = run.go();
        if pin_is_right {
            assert_eq!(outcome.code, Some(0), "{}", outcome.text());
            assert!(run.install_dir().join("req").is_file());
        } else {
            failed(&outcome, "shasum against a wrong pin");
            assert!(
                outcome.stderr.contains("SHA-256 mismatch"),
                "{}",
                outcome.text()
            );
            assert_nothing_installed(&run.install_dir(), "shasum mismatch");
        }
    }
}

#[test]
fn n_with_no_sha_tool_at_all_the_install_refuses_before_downloading() {
    let scratch = Scratch::new("no-sha-tool");
    let (server, hash) = serving_good(REV);
    let mut run = Run::new(&scratch, &server, &hash);
    run.only_tools = Some(BASE_TOOLS.to_vec());
    let outcome = run.go();
    failed(&outcome, "no SHA-256 tool");
    assert!(
        outcome.stderr.contains("SHA-256 tool"),
        "the refusal names the missing tool:\n{}",
        outcome.text()
    );
    assert!(server.requests().is_empty());
}

#[test]
fn m_by_default_only_https_is_spoken_so_the_token_is_never_sent_in_the_clear() {
    // `REQ_CURL_PROTO` unset: the script's own default applies. An `http://` base is then a
    // refusal by curl itself, and the server never sees a request (or the token).
    let scratch = Scratch::new("https-only");
    let (server, hash) = serving_good(REV);
    let mut run = Run::new(&scratch, &server, &hash);
    run.token = Some(TOKEN);
    run.proto = None;
    let outcome = run.go();

    failed(&outcome, "an http:// base without the test-only override");
    assert!(
        server.requests().is_empty(),
        "no request reaches a plain-http server by default: {:?}",
        server.requests()
    );
    assert!(!outcome.text().contains(TOKEN));
    assert_nothing_installed(&run.install_dir(), "an http base");
}

#[test]
fn l_the_shipped_script_opens_with_the_pin_table_and_pins_its_transport() {
    let source = std::fs::read_to_string(repo_root().join(SCRIPT)).expect("reads the script");
    let lines: Vec<&str> = source.lines().collect();
    assert_eq!(lines[0], "#!/usr/bin/env bash", "a bash script");
    let head: Vec<&str> = lines.iter().take(8).copied().collect();
    for key in [
        "REV=",
        "SHA256_x86_64_unknown_linux_gnu=",
        "SHA256_aarch64_apple_darwin=",
    ] {
        assert!(
            head.iter().any(|line| line.starts_with(key)),
            "the pin table is the first thing in the file ({key} missing from its first lines)"
        );
    }
    let mode = std::fs::metadata(repo_root().join(SCRIPT))
        .expect("stat")
        .permissions()
        .mode();
    assert!(mode & 0o111 != 0, "the script is executable");
    for required in [
        "set -eu",
        "--proto",
        "--tlsv1.2",
        "-fsSL",
        "REQ_RELEASE_BASE",
        "REQ_CURL_PROTO",
        "REQ_INSTALL_DIR",
        "https://github.com/autox-my/req/releases/download",
    ] {
        assert!(
            source.contains(required),
            "{SCRIPT} must carry {required:?}"
        );
    }
    // The transport is pinned on the code lines, not in a comment that merely names it.
    let code: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();
    for required in [
        "curl --proto \"$proto\" --tlsv1.2 -fsSL -H @-",
        "proto=${install_proto:-=https}",
        "base=${install_base:-https://github.com/autox-my/req/releases/download}",
        // The header is conditional: only a non-empty token produces one.
        "if [ -n \"${REQ_RELEASE_TOKEN:-}\" ]; then printf 'Authorization: token %s\\n' \"$REQ_RELEASE_TOKEN\"; fi",
    ] {
        assert!(
            code.iter().any(|line| line.contains(required)),
            "{SCRIPT} must carry {required:?} in code"
        );
    }
    // The two test-only overrides are documented as such, in the header.
    let header: String = lines
        .iter()
        .take_while(|line| line.starts_with('#') || line.contains('='))
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        header.contains("test-only") || source.contains("test-only"),
        "the overrides are documented as test-only"
    );
}

/// `cargo install --git` on a repository with several binary packages refuses without a package
/// name (issue #11: req, req-paraphrase and req-tier2). Every source-install command in the docs an
/// adopter copies must therefore name `req`, as its own argument, after the flags.
#[test]
fn every_source_install_command_in_the_docs_names_the_req_package() {
    let mut seen = 0;
    for rel in ["README.md", "docs/adopting.md", "docs/releasing.md"] {
        let text = std::fs::read_to_string(repo_root().join(rel)).expect("a doc reads");
        let flat = text.replace("\\\n", " ");
        for line in flat.lines().filter(|l| l.contains("cargo install --git")) {
            seen += 1;
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let at = tokens
                .iter()
                .position(|t| *t == "install")
                .expect("an install command");
            assert!(
                tokens[at..].contains(&"req"),
                "{rel}: `cargo install --git` must name the package `req` (the repository has three \
                 binary packages and cargo refuses without one):\n{line}"
            );
        }
    }
    assert!(
        seen >= 3,
        "the three docs carry a source install; found {seen}"
    );
}

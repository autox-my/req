#!/usr/bin/env python3
"""tools/junit_case.py — run one command and record it as one JUnit <testcase> (ADR 0024 §3).

A starting point an adopter copies and adjusts, like `tools/adopt.py` — not a req subcommand: req
runs nothing and writes no report. Python 3 standard library only, POSIX.

    python3 tools/junit_case.py --report <file> --classname <C> --name <N> [--timeout <s>] [--fresh]
                                -- <command> [args...]
    python3 tools/junit_case.py --self-check

The case's id is `<C>::<N>` — what a claim's `tests:` entry names and `req gate --test-report <file>`
reads. The command is run as given (no shell); its stdin, stdout and stderr are the adapter's own,
inherited untouched, and the adapter prints nothing of its own on success.

Mapping (ADR 0024 §3), and the adapter's own exit, which is the command's, so wrapping a check never
hides its failure:

    command exit 0                       passed (a bare <testcase>)                       exit 0
    command exit 1                       <failure message="exit 1: <command>">            exit 1
    command exit 3                       <skipped message="UNVERIFIABLE (exit 3): …">     exit 3
    any other exit status N              <error message="exit N: <command>">              exit N
    killed by signal N                   <error message="killed by signal N (SIG…): …">   exit 128+N
    executable not found                 <error message="cannot start: …">                exit 127
    cannot be executed (permissions …)   <error message="cannot start: …">                exit 126
    --timeout <s> elapsed                <error message="timed out after <s>s: …">        exit 124
    the adapter itself got SIGINT/TERM/HUP   <error message="interrupted by SIG…: …">     exit 128+N

Exit 3 is `skipped` on purpose and its consequence is stated, not hidden: `req gate` reads a claim whose
linked tests are all skipped as FAIL (exit 1, ADR 0019 §5) — never a pass, and not exit 3 (ADR 0024 §4).
Exit 2 is ambiguous by construction: the adapter's own usage and report refusals exit 2 *before* any
command runs and print a `junit_case:` line on stderr; a command that itself exits 2 is an `<error>`
and also exits 2. Read the stderr line to tell them apart. If the command ran but the case cannot be
recorded, the adapter says so on stderr and exits with the command's status (2 when that was 0).

The report (rewritten on every run; one adapter at a time per report is the contract, and a lock on
the report's directory serialises the read-modify-write where the platform allows it):

    <testsuites><testsuite name="junit_case" tests=… failures=… errors=… skipped=…>
      <testcase classname="…" name="…" time="…"/> …

* A case with the same `classname::name` **replaces** its previous row (in place); other cases are kept.
  A run that is interrupted or killed is recorded as an `<error>`, so an earlier pass cannot outlive it.
* The write is **atomic**: a temp file in the report's own directory, then `os.replace`. The directory
  is created if missing. A report path must be a regular file: a FIFO, a directory or a symlink is
  refused (never opened, so a FIFO cannot hang the adapter; stdin is never read).
* An existing report that is not parseable XML, or not exactly this shape (the root `testsuites`
  holding the one `junit_case` suite, `testcase` rows with `classname`, `name` and optional `time`
  and at most one `failure`/`skipped`/`error` child; no DTD), is **refused**: exit 2, the file left
  byte-identical, **the command not run**. `--fresh` starts a new report instead, dropping whatever
  was there (a valid report's other rows too).
* Arguments are validated before any file is touched, and a usage error never mutates a report:
  empty or control-character `--classname`/`--name`, an empty or option-shaped `--report` (a leading
  `-`), a repeated or unknown option, a missing `--` or command, and a `--timeout` that is not a
  finite number of seconds in (0, 10000000] are exit 2.
* `--timeout` puts the command in its own process group and kills the whole group (SIGKILL) when it
  expires; without it the command stays in the adapter's group, so a terminal's Ctrl-C reaches both.

Several stacks, several reports: each case can have its own report file; hand every one to
`req gate --test-report <a> --test-report <b>` (ADR 0024 §2) rather than merging them here.

`--self-check` runs the adapter's own table in a temp dir (no other filesystem, no network) and exits
0 when every case answers as recorded, 1 otherwise. `--help` prints this page.
"""
from __future__ import annotations

import collections
import math
import os
import shlex
import signal
import stat
import subprocess
import sys
import tempfile
import time
import unicodedata
import xml.etree.ElementTree as ET

try:  # POSIX; absent elsewhere, where the lock is simply not taken.
    import fcntl
except ImportError:  # pragma: no cover
    fcntl = None

SUITE = "junit_case"
KINDS = ("failure", "skipped", "error")
MAX_REPORT_BYTES = 16 * 1024 * 1024
MAX_TIMEOUT = 10_000_000.0
MAX_MESSAGE = 2000
GRACE_SECONDS = 5

Row = collections.namedtuple("Row", "classname name kind message time")


class Usage(Exception):
    """A malformed invocation: exit 2, before any file is touched."""


class Refused(Exception):
    """A report (or its location) the adapter will not read or replace: exit 2, nothing written."""


class _Interrupted(BaseException):
    """SIGTERM/SIGHUP delivered to the adapter. A BaseException so no `except Exception` eats it."""

    def __init__(self, signum):
        super().__init__(signum)
        self.signum = signum


# --- the pure half -----------------------------------------------------------


def xml_legal(char):
    """True when `char` may appear in an XML 1.0 document at all."""
    code = ord(char)
    return (
        code in (0x9, 0xA, 0xD)
        or 0x20 <= code <= 0xD7FF
        or 0xE000 <= code <= 0xFFFD
        or 0x10000 <= code <= 0x10FFFF
    )


def clean(text, limit=None):
    """`text` with every character XML 1.0 cannot carry written out as `\\xNN`/`\\uNNNN`; with a
    `limit`, also capped (ids are never capped: a cut id would name another test)."""
    out = []
    for char in text:
        if xml_legal(char):
            out.append(char)
        else:
            code = ord(char)
            out.append(f"\\x{code:02x}" if code < 0x100 else f"\\u{code:04x}")
    cleaned = "".join(out)
    if limit is not None and len(cleaned) > limit:
        cleaned = cleaned[:limit] + "…[truncated]"
    return cleaned


def attr(value):
    """An attribute value, escaped; newline, carriage return and tab survive as character references."""
    table = {"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "\n": "&#10;", "\r": "&#13;", "\t": "&#9;"}
    return "".join(table.get(char, char) for char in clean(value))


def check_id_part(value, label):
    if not value.strip():
        raise Usage(f"{label} is empty: the id is `classname::name` and an empty part names no test")
    for char in value:
        if unicodedata.category(char) == "Cc" or not xml_legal(char):
            raise Usage(f"{label} contains a control character (U+{ord(char):04X}); it cannot name a test")


def case_id(classname, name):
    return f"{classname}::{name}"


Options = collections.namedtuple("Options", "report classname name timeout fresh command")
_VALUED = ("--report", "--classname", "--name", "--timeout")


def parse_args(argv):
    """The invocation, validated; `Usage` otherwise. Touches nothing."""
    if not argv:
        raise Usage("no arguments (try --help)")
    given = {}
    fresh = False
    command = None
    index = 0
    while index < len(argv):
        arg = argv[index]
        if arg == "--":
            command = argv[index + 1 :]
            break
        if arg == "--fresh":
            if fresh:
                raise Usage("`--fresh` is given twice")
            fresh = True
            index += 1
            continue
        key, equals, value = arg.partition("=") if arg.startswith("--") else (arg, "", "")
        if key not in _VALUED:
            raise Usage(f"unknown argument `{arg}` (the command goes after `--`)")
        if key in given:
            raise Usage(f"`{key}` is given twice")
        if not equals:
            index += 1
            if index >= len(argv) or argv[index] == "--":
                raise Usage(f"`{key}` needs a value")
            value = argv[index]
        given[key] = value
        index += 1
    for key in _VALUED[:3]:
        if key not in given:
            raise Usage(f"`{key}` is required")
    if command is None:
        raise Usage("missing `--` before the command")
    if not command:
        raise Usage("no command after `--`")
    report = given["--report"]
    if not report or report.startswith("-"):
        raise Usage("`--report` needs a file path (empty or option-shaped values are refused; use ./-name for such a file)")
    if os.path.basename(report) in ("", ".", ".."):
        raise Usage(f"`--report {report}` does not name a file")
    check_id_part(given["--classname"], "`--classname`")
    check_id_part(given["--name"], "`--name`")
    timeout = None
    if "--timeout" in given:
        try:
            timeout = float(given["--timeout"])
        except ValueError:
            raise Usage(f"`--timeout {given['--timeout']}` is not a number of seconds") from None
        if not math.isfinite(timeout) or timeout <= 0 or timeout > MAX_TIMEOUT:
            raise Usage(f"`--timeout` must be a finite number of seconds greater than 0 and at most {MAX_TIMEOUT:g}")
    return Options(report, given["--classname"], given["--name"], timeout, fresh, command)


def signal_name(number):
    try:
        return signal.Signals(number).name
    except ValueError:
        return f"signal {number}"


def classify(returncode, command_text):
    """(kind, message, adapter exit) for a finished command; `kind` None means passed."""
    if returncode == 0:
        return None, "", 0
    if returncode < 0:
        number = -returncode
        return "error", f"killed by signal {number} ({signal_name(number)}): {command_text}", 128 + number
    if returncode == 1:
        return "failure", f"exit 1: {command_text}", 1
    if returncode == 3:
        return "skipped", f"UNVERIFIABLE (exit 3): {command_text}", 3
    return "error", f"exit {returncode}: {command_text}", returncode


def render(rows):
    """The report's bytes for `rows`."""
    counts = {kind: sum(1 for row in rows if row.kind == kind) for kind in KINDS}
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        "<testsuites>",
        f'  <testsuite name="{SUITE}" tests="{len(rows)}" failures="{counts["failure"]}" '
        f'errors="{counts["error"]}" skipped="{counts["skipped"]}">',
    ]
    for row in rows:
        head = f'    <testcase classname="{attr(row.classname)}" name="{attr(row.name)}"'
        if row.time is not None:
            head += f' time="{attr(row.time)}"'
        if row.kind is None:
            lines.append(head + "/>")
        else:
            lines.append(head + ">")
            lines.append(f'      <{row.kind} message="{attr(row.message)}"/>')
            lines.append("    </testcase>")
    lines += ["  </testsuite>", "</testsuites>", ""]
    return "\n".join(lines).encode("utf-8")


def _blank(text):
    return text is None or not text.strip()


def parse_rows(data):
    """The rows of a report in exactly this shape; `Refused` (naming why) otherwise."""
    if b"<!DOCTYPE" in data or b"<!ENTITY" in data:
        raise Refused("it carries a DTD, which this adapter never writes")
    try:
        root = ET.fromstring(data)
    except (ET.ParseError, ValueError, UnicodeError) as error:
        raise Refused(f"it is not well-formed XML ({error})") from None
    if root.tag != "testsuites" or root.attrib or not _blank(root.text):
        raise Refused("its root is not a bare `<testsuites>`")
    suites = list(root)
    if len(suites) != 1 or suites[0].tag != "testsuite" or suites[0].get("name") != SUITE:
        raise Refused(f'it is not exactly one `<testsuite name="{SUITE}">`')
    suite = suites[0]
    if not set(suite.attrib) <= {"name", "tests", "failures", "errors", "skipped"} or not _blank(suite.text):
        raise Refused("its `<testsuite>` carries attributes or text this adapter does not write")
    rows = []
    seen = set()
    for case in suite:
        if case.tag != "testcase" or not _blank(case.text) or not _blank(case.tail):
            raise Refused(f"it holds `<{case.tag}>` where only `<testcase>` rows belong")
        if not set(case.attrib) <= {"classname", "name", "time"} or "classname" not in case.attrib or "name" not in case.attrib:
            raise Refused("a `<testcase>` lacks classname/name or carries other attributes")
        classname, name = case.get("classname"), case.get("name")
        try:
            check_id_part(classname, "a `classname`")
            check_id_part(name, "a `name`")
        except Usage as error:
            raise Refused(str(error)) from None
        stamp = case.get("time")
        if stamp is not None:
            try:
                float(stamp)
            except ValueError:
                raise Refused(f"`time` of `{case_id(classname, name)}` is not a number") from None
        children = list(case)
        kind, message = None, ""
        if len(children) > 1:
            raise Refused(f"`{case_id(classname, name)}` has more than one outcome element")
        if children:
            child = children[0]
            if child.tag not in KINDS or not set(child.attrib) <= {"message"} or len(child) or not _blank(child.text):
                raise Refused(f"`{case_id(classname, name)}` has an outcome element this adapter does not write")
            kind, message = child.tag, child.get("message", "")
        identity = case_id(classname, name)
        if identity in seen:
            raise Refused(f"`{identity}` appears twice")
        seen.add(identity)
        rows.append(Row(classname, name, kind, message, stamp))
    return rows


def merge_row(rows, row):
    """`rows` with `row` replacing the one of the same id (in place), else appended."""
    identity = case_id(row.classname, row.name)
    merged = list(rows)
    for index, old in enumerate(merged):
        if case_id(old.classname, old.name) == identity:
            merged[index] = row
            return merged
    merged.append(row)
    return merged


# --- the filesystem half -----------------------------------------------------


def inspect_target(path):
    """True when `path` exists as a regular file, False when absent; `Refused` for anything else.
    Uses lstat: a symlink is refused rather than followed, so `os.replace` never swaps one out."""
    try:
        info = os.lstat(path)
    except FileNotFoundError:
        return False
    except OSError as error:
        raise Refused(f"cannot examine `{path}`: {error.strerror}") from None
    if stat.S_ISLNK(info.st_mode):
        raise Refused(f"`{path}` is a symbolic link; pass the file it points to")
    if not stat.S_ISREG(info.st_mode):
        raise Refused(f"`{path}` is not a regular file")
    return True


def read_rows(path):
    """The rows of the existing report at `path`; `Refused` if it cannot be read or is not this shape.
    Opened non-blocking and without following links, and judged by `fstat` on the descriptor, so a
    file swapped for a FIFO after `inspect_target` still cannot hang this."""
    flags = os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise Refused(f"cannot read `{path}`: {error.strerror}") from None
    try:
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode):
            raise Refused(f"`{path}` is not a regular file")
        if info.st_size > MAX_REPORT_BYTES:
            raise Refused(f"`{path}` is larger than {MAX_REPORT_BYTES} bytes")
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            data = handle.read(MAX_REPORT_BYTES + 1)
    finally:
        os.close(descriptor)
    try:
        return parse_rows(data)
    except Refused as error:
        raise Refused(f"`{path}`: {error}") from None


def report_directory(path):
    return os.path.dirname(os.path.abspath(path))


def write_atomic(path, data):
    """Write `data` as `path` through a temp file in the same directory and `os.replace`."""
    directory = report_directory(path)
    if os.path.lexists(path):
        mode = stat.S_IMODE(os.lstat(path).st_mode)
    else:
        umask = os.umask(0)
        os.umask(umask)
        mode = 0o666 & ~umask
    descriptor, temporary = tempfile.mkstemp(dir=directory, prefix=".junit_case-", suffix=".tmp")
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise
    try:  # make the rename durable; best effort, the report is already in place
        directory_descriptor = os.open(directory, os.O_RDONLY)
        try:
            os.fsync(directory_descriptor)
        finally:
            os.close(directory_descriptor)
    except OSError:
        pass


def preflight(options):
    """Everything that can be known before the command runs, read-only except for creating the
    report's directory: the target is a regular file or absent, an existing report parses (unless
    `--fresh`), and the directory exists and is writable."""
    exists = inspect_target(options.report)
    if exists and not options.fresh:
        try:
            read_rows(options.report)
        except Refused as error:
            raise Refused(f"{error}; the command was not run, nothing was written (--fresh starts a new report)") from None
    directory = report_directory(options.report)
    try:
        os.makedirs(directory, exist_ok=True)
    except OSError as error:
        raise Refused(f"cannot create `{directory}`: {error.strerror}") from None
    if not os.access(directory, os.W_OK | os.X_OK):
        raise Refused(f"`{directory}` is not writable")


def record(options, row):
    """Read-modify-write the report under a lock on its directory, atomically."""
    directory = report_directory(options.report)
    try:
        descriptor = os.open(directory, os.O_RDONLY)
    except OSError as error:
        raise Refused(f"cannot open `{directory}`: {error.strerror}") from None
    try:
        if fcntl is not None:
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX)
            except OSError:
                pass  # advisory; a filesystem without locks gets the atomic write alone
        exists = inspect_target(options.report)
        rows = read_rows(options.report) if exists and not options.fresh else []
        write_atomic(options.report, render(merge_row(rows, row)))
    except OSError as error:
        raise Refused(f"cannot write `{options.report}`: {error.strerror}") from None
    finally:
        os.close(descriptor)


# --- the command ---------------------------------------------------------------


def _signal_group_or_process(process, own_group, number):
    try:
        if own_group:
            os.killpg(process.pid, number)
        else:
            process.send_signal(number)
    except (ProcessLookupError, PermissionError):
        pass


def _stop(process, own_group, number):
    """Ask the command to stop, then insist after a grace period; always reaps it."""
    _signal_group_or_process(process, own_group, number)
    try:
        process.wait(timeout=GRACE_SECONDS)
    except (subprocess.TimeoutExpired, KeyboardInterrupt, _Interrupted):
        _signal_group_or_process(process, own_group, signal.SIGKILL)
        process.wait()


def run_command(command, timeout):
    """Run `command`; (kind, message, adapter exit, elapsed seconds). `kind` None means passed."""
    text = shlex.join(command)
    own_group = timeout is not None
    started = time.monotonic()
    handlers = {}

    def interrupted(signum, _frame):
        raise _Interrupted(signum)

    for number in (signal.SIGTERM, signal.SIGHUP):
        handlers[number] = signal.signal(number, interrupted)
    process = None
    try:
        try:
            process = subprocess.Popen(command, start_new_session=own_group)
            returncode = process.wait(timeout=timeout)
            kind, message, status = classify(returncode, text)
        except subprocess.TimeoutExpired:
            _signal_group_or_process(process, own_group, signal.SIGKILL)
            process.wait()
            kind, message, status = "error", f"timed out after {timeout:g}s: {text}", 124
        except (KeyboardInterrupt, _Interrupted) as caught:
            number = caught.signum if isinstance(caught, _Interrupted) else signal.SIGINT
            if process is not None:
                _stop(process, own_group, signal.SIGTERM)
            kind, message, status = "error", f"interrupted by {signal_name(number)}: {text}", 128 + number
        except (FileNotFoundError, NotADirectoryError) as error:
            kind, message, status = "error", f"cannot start: {text}: {error.strerror}", 127
        except OSError as error:
            kind, message, status = "error", f"cannot start: {text}: {error.strerror or error}", 126
    finally:
        for number, handler in handlers.items():
            signal.signal(number, handler)
    return kind, message, status, time.monotonic() - started


def complain(message):
    print(f"junit_case: {message}", file=sys.stderr)


def main(argv):
    if argv in (["--help"], ["-h"]):
        print(__doc__)
        return 0
    if argv[:1] == ["--self-check"]:
        if len(argv) != 1:
            complain("`--self-check` takes no other arguments")
            return 2
        return self_check()
    try:
        options = parse_args(argv)
        preflight(options)
    except (Usage, Refused) as error:
        complain(str(error))
        return 2
    kind, message, status, elapsed = run_command(options.command, options.timeout)
    row = Row(options.classname, options.name, kind, clean(message, MAX_MESSAGE), f"{elapsed:.3f}")
    try:
        record(options, row)
    except Refused as error:
        complain(f"the command ran (exit {status}) but the case was not recorded: {error}")
        return status if status != 0 else 2
    return status


# --- the self-check ----------------------------------------------------------------


def self_check():
    """The adapter's own table, run in a temp dir through the real script. 0 = all as recorded."""
    failures = []
    count = [0]
    me = os.path.abspath(__file__)

    def check(label, ok, detail=""):
        count[0] += 1
        if not ok:
            failures.append(f"{label}: {detail}")

    with tempfile.TemporaryDirectory(prefix="junit_case-selfcheck-") as scratch:

        def invoke(args):
            result = subprocess.run([sys.executable, me, *args], capture_output=True, cwd=scratch, timeout=120)
            return result.returncode, result.stderr.decode("utf-8", "replace")

        def read(path):
            with open(path, "rb") as handle:
                return handle.read()

        def case(report, name, command, extra=()):
            return invoke(["--report", report, "--classname", "self", "--name", name, *extra, "--", *command])

        # the exit mapping, from the pure half
        for code, kind, status in ((0, None, 0), (1, "failure", 1), (3, "skipped", 3), (2, "error", 2), (255, "error", 255), (-9, "error", 137)):
            got = classify(code, "cmd")
            check(f"classify({code})", got[0] == kind and got[2] == status, repr(got))
        # parse_args: what is refused, before any file
        for label, argv in (
            ("empty", []),
            ("no separator", ["--report", "r", "--classname", "c", "--name", "n", "x"]),
            ("empty name", ["--report", "r", "--classname", "c", "--name", "", "--", "x"]),
            ("control in classname", ["--report", "r", "--classname", "a\tb", "--name", "n", "--", "x"]),
            ("option-shaped report", ["--report", "-r", "--classname", "c", "--name", "n", "--", "x"]),
            ("repeated", ["--report", "r", "--report", "r", "--classname", "c", "--name", "n", "--", "x"]),
            ("bad timeout", ["--report", "r", "--classname", "c", "--name", "n", "--timeout", "nan", "--", "x"]),
        ):
            try:
                parse_args(argv)
                check(f"parse_args {label}", False, "accepted")
            except Usage:
                check(f"parse_args {label}", True)
        accepted = parse_args(["--report=r", "--classname=c", "--name=n", "--timeout=1.5", "--fresh", "--", "-x", "--", "y"])
        check("parse_args accepts the = form and keeps a command's own `--`", accepted.command == ["-x", "--", "y"] and accepted.timeout == 1.5 and accepted.fresh)
        # the report round-trips: render -> parse_rows, awkward characters included
        awkward = [
            Row("a::b", 'p[a&b <c> "d"\' e]', "failure", "exit 1: x\nnew line\ttab \x07 bell", "0.001"),
            Row("c", "plain", None, "", None),
        ]
        again = parse_rows(render(awkward))
        check("render/parse round trip", [(r.classname, r.name, r.kind) for r in again] == [(r.classname, r.name, r.kind) for r in awkward], repr(again))
        check("a control character in a message is written out, not dropped", "\\x07" in again[0].message, repr(again[0].message))
        for label, text in (("garbage", b"nope"), ("empty", b""), ("a DTD", b'<!DOCTYPE a [<!ENTITY x "y">]><testsuites/>'), ("another suite", b'<testsuites><testsuite name="x"/></testsuites>')):
            try:
                parse_rows(text)
                check(f"parse_rows refuses {label}", False, "accepted")
            except Refused:
                check(f"parse_rows refuses {label}", True)

        # the real script, real commands
        report = os.path.join(scratch, "out", "report.xml")
        for code in (0, 1, 3, 2, 7):
            got, _ = case(report, "exit", [sys.executable, "-c", f"raise SystemExit({code})"])
            check(f"adapter exit equals command exit {code}", got == code, str(got))
        check("a rerun replaced, not appended", len(parse_rows(read(report))) == 1)
        case(report, "second", [sys.executable, "-c", "pass"])
        check("two cases accumulate", len(parse_rows(read(report))) == 2)
        got, _ = case(report, "gone", ["junit-case-no-such-command"])
        check("a missing executable exits 127", got == 127, str(got))
        got, _ = case(report, "slow", [sys.executable, "-c", "import time; time.sleep(60)"], ["--timeout", "0.5"])
        check("a timeout exits 124", got == 124, str(got))
        got, _ = case(report, "sig", [sys.executable, "-c", "import os, signal; os.kill(os.getpid(), signal.SIGKILL)"])
        check("a signalled command exits 128+N", got == 137, str(got))
        rows = {case_id(r.classname, r.name): r for r in parse_rows(read(report))}
        check("every outcome was recorded", len(rows) == 5 and rows["self::exit"].kind == "error" and rows["self::second"].kind is None, repr(sorted(rows)))

        # refusal leaves the report byte-identical and the command unrun; --fresh replaces
        bad = os.path.join(scratch, "bad.xml")
        with open(bad, "wb") as handle:
            handle.write(b"<testsuites>")
        marker = os.path.join(scratch, "ran")
        got, message = case(bad, "c", [sys.executable, "-c", f"open({marker!r}, 'w').close()"])
        check("an unparsable report is refused", got == 2 and "--fresh" in message, f"{got} {message}")
        check("...and left byte-identical", read(bad) == b"<testsuites>")
        check("...and the command did not run", not os.path.exists(marker))
        got, _ = case(bad, "c", [sys.executable, "-c", "pass"], ["--fresh"])
        check("--fresh replaces it", got == 0 and len(parse_rows(read(bad))) == 1)

        # usage never touches a file
        before = read(report)
        got, _ = invoke(["--report", report, "--classname", "", "--name", "n", "--", "true"])
        check("a usage error exits 2 and leaves the report alone", got == 2 and read(report) == before)
        leftovers = [name for name in os.listdir(os.path.dirname(report)) if name != "report.xml"]
        check("no temp file is left beside the report", not leftovers, repr(leftovers))

    print(f"junit_case --self-check: {count[0]} case(s), {len(failures)} failure(s)")
    for line in failures:
        print(f"  FAIL {line}")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

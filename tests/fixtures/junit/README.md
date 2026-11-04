# JUnit fixtures

Real output of `pytest 9.2.0` (Python 3.15.2, macOS) with `--junitxml`, recorded 2026-11-04 for issue #64
and ADR 0024. Only the machine-specific parts were rewritten: the scratch path became `/work`, the host
name `host`, and every timestamp a fixed one. Nothing else was edited, so the shapes are pytest's own.

| fixture | produced by | what it holds |
| --- | --- | --- |
| `pytest-mixed.xml` | `pytest -q --junitxml=… tests` over `pytest_probe/test_probe.py` | a pass, a failure (message with `<`, `&`, `>` and newline entities), `@skip`, non-strict `xfail`, strict XPASS, three parametrized ids (one with a space), a setup error, a class method carrying a `<properties>` block |
| `pytest-collection-error.xml` | `pytest -q --junitxml=… coll` with `pytest_probe/test_broken.py` importing a missing module | pytest interrupts the run and writes one `<testcase classname="" name="coll.test_broken">` carrying an `<error>` |
| `pytest-param-escapes.xml` | `pytest -q --junitxml=… coll/test_param.py` over `pytest_probe/test_param.py` | parametrized ids `a&b`, `x<y`, `q"r`, written as `&amp;`, `&lt;`, `&quot;` in the `name` attribute |

The sources are beside them (`pytest_probe/`) so the reports can be regenerated; the expectations in the
tests that read them come from pytest's documented meaning of each case, not from `req`'s output.

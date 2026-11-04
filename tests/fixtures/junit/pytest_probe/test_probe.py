import pytest

def test_passes():
    assert 1 + 1 == 2

def test_fails():
    assert "a<b & c>" == "x"

@pytest.mark.skip(reason="not today")
def test_skipped():
    pass

@pytest.mark.xfail(reason="known")
def test_xfail():
    assert False

@pytest.mark.xfail(strict=True)
def test_xpass_strict():
    assert True

@pytest.mark.parametrize("n", [1, 2, "a b"])
def test_param(n):
    assert n

@pytest.fixture
def broken():
    raise RuntimeError("setup blew up")

def test_setup_error(broken):
    pass

class TestGroup:
    def test_in_class(self, record_property):
        record_property("req", "REQ-0003")

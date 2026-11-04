import pytest
@pytest.mark.parametrize("v", ["a&b","x<y","q\"r"])
def test_p(v):
    pass

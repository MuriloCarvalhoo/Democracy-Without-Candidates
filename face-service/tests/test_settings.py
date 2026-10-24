import pytest

from dwc_face.settings import SettingsError, from_env

from conftest import KEY


@pytest.fixture
def env(tmp_path):
    key_file = tmp_path / "face.key"
    key_file.write_text(KEY + "\n")
    return {"DWC_FACE_KEY_FILE": str(key_file)}


def test_defaults_bind_loopback(env):
    s = from_env(env)
    assert (s.host, s.port, s.engine, s.max_distance) == ("127.0.0.1", 8001, "deepface", 0.30)
    assert s.max_image_bytes == 2 * 1024 * 1024
    assert s.key == KEY


@pytest.mark.parametrize("host", ["127.0.0.1", "::1"])
def test_loopback_hosts_accepted(env, host):
    assert from_env({**env, "DWC_FACE_HOST": host}).host == host


@pytest.mark.parametrize("host", ["0.0.0.0", "::", "10.0.0.5", "192.168.1.2", "localhost", ""])
def test_non_loopback_bind_refused(env, host):
    with pytest.raises(SettingsError):
        from_env({**env, "DWC_FACE_HOST": host})


def test_key_file_required_and_strong(env, tmp_path):
    with pytest.raises(SettingsError, match="DWC_FACE_KEY_FILE"):
        from_env({})
    with pytest.raises(SettingsError):
        from_env({"DWC_FACE_KEY_FILE": str(tmp_path / "missing")})
    short = tmp_path / "short.key"
    short.write_text("short")
    with pytest.raises(SettingsError, match="at least"):
        from_env({"DWC_FACE_KEY_FILE": str(short)})


def test_fake_engine_requires_explicit_test_mode(env):
    with pytest.raises(SettingsError, match="TEST_MODE"):
        from_env({**env, "DWC_FACE_ENGINE": "fake"})
    assert from_env({**env, "DWC_FACE_ENGINE": "fake", "DWC_FACE_TEST_MODE": "1"}).engine == "fake"
    with pytest.raises(SettingsError):
        from_env({**env, "DWC_FACE_ENGINE": "other"})


@pytest.mark.parametrize(
    "name,value",
    [
        ("DWC_FACE_MAX_DISTANCE", "-0.1"),
        ("DWC_FACE_MAX_DISTANCE", "3"),
        ("DWC_FACE_MAX_DISTANCE", "nan"),
        ("DWC_FACE_PORT", "0"),
        ("DWC_FACE_PORT", "70000"),
        ("DWC_FACE_CONCURRENCY", "0"),
        ("DWC_FACE_MAX_IMAGE_BYTES", "abc"),
    ],
)
def test_invalid_numbers_fail(env, name, value):
    with pytest.raises(SettingsError):
        from_env({**env, name: value})


def test_threshold_configurable(env):
    assert from_env({**env, "DWC_FACE_MAX_DISTANCE": "0.25"}).max_distance == 0.25

import logging

import numpy as np
import pytest

from dwc_face import trace
from dwc_face.app import create_app
from dwc_face.engine import FakeEngine, decode_image
from dwc_face.settings import SettingsError, from_env

from conftest import AUTH, KEY, b64, image_bytes, make_settings


@pytest.fixture
def tracing(caplog):
    trace.enable(True)
    caplog.set_level(logging.DEBUG, logger="dwc_face.trace")
    yield caplog
    trace.enable(False)


def test_disabled_by_default_logs_nothing(caplog):
    caplog.set_level(logging.DEBUG, logger="dwc_face.trace")
    decode_image(image_bytes())
    assert not trace.enabled()
    assert caplog.records == []


def test_function_call_logs_module_name_and_sizes_not_bytes(tracing):
    data = image_bytes()
    decode_image(data)
    text = tracing.text
    assert "call dwc_face.engine.decode_image(data=<redacted bytes len=" in text
    assert f"len={len(data)}" in text
    assert "return dwc_face.engine.decode_image -> <ndarray shape=(48, 48, 3)>" in text
    assert repr(data[:16]) not in text


def test_class_methods_log_class_and_hide_embeddings(tracing):
    engine = FakeEngine()
    rgb = decode_image(image_bytes())
    embedding = engine.embed(rgb)
    text = tracing.text
    assert "call dwc_face.engine.FakeEngine.embed(rgb=<redacted ndarray" in text
    assert "-> <list len=512>" in text
    assert repr(embedding[0]) not in text


def test_exceptions_are_logged_by_type_and_reraised(tracing):
    with pytest.raises(Exception):
        decode_image(b"not an image")
    assert "raise dwc_face.engine.decode_image: Unusable" in tracing.text


def test_http_request_traced_without_image_key_or_embedding(tracing):
    client = create_app(make_settings(), FakeEngine()).test_client()
    data = image_bytes(seed=7)
    encoded = b64(data)
    res = client.post("/v1/embed", json={"image_b64": encoded, "challenge": "c-1"}, headers=AUTH)
    assert res.status_code == 200
    embedding = res.get_json()["embedding"]
    text = tracing.text
    assert "call dwc_face.app.embed()" in text
    assert "dwc_face.app.run_engine" in text
    for secret in (encoded[:40], KEY, repr(embedding[0])):
        assert secret not in text


def test_summarize_redacts_sensitive_dict_keys():
    text = trace.summarize({"image_b64": "QUJD" * 100, "embedding": [0.1] * 512, "challenge": "c"})
    assert "QUJD" not in text and "0.1" not in text
    assert "'challenge': 'c'" in text
    assert trace.summarize(np.zeros((2, 2))) == "<ndarray shape=(2, 2)>"
    assert trace.summarize("x" * 500) == "<str len=500>"


def test_debug_setting_is_validated(tmp_path):
    key_file = tmp_path / "face.key"
    key_file.write_text(KEY)
    env = {"DWC_FACE_KEY_FILE": str(key_file)}
    assert from_env(env).debug is False
    assert from_env({**env, "DWC_FACE_DEBUG": "1"}).debug is True
    assert from_env({**env, "DWC_FACE_DEBUG": "0"}).debug is False
    with pytest.raises(SettingsError, match="DWC_FACE_DEBUG"):
        from_env({**env, "DWC_FACE_DEBUG": "yes"})

"""DeepFaceEngine logic against a stub DeepFace module (no TensorFlow needed)."""

from io import BytesIO

import numpy as np
import pytest
from PIL import Image

from dwc_face.engine import DeepFaceEngine, Unusable, decode_image

from conftest import image_bytes


class StubDeepFace:
    def __init__(self, result=None, error=None):
        self.result, self.error, self.calls = result, error, []

    def represent(self, **kwargs):
        self.calls.append(kwargs)
        if self.error:
            raise self.error
        return self.result


def test_engine_passes_in_memory_bgr_array_not_a_path():
    stub = StubDeepFace(result=[{"embedding": [0.5] * 512}])
    rgb = decode_image(image_bytes(corner=(10, 20, 30)))
    assert DeepFaceEngine(stub).embed(rgb) == [0.5] * 512
    call = stub.calls[0]
    assert isinstance(call["img_path"], np.ndarray)
    assert tuple(call["img_path"][0, 0]) == (30, 20, 10)  # RGB -> BGR
    assert call["model_name"] == "Facenet512"
    assert call["enforce_detection"] is True
    assert call["detector_backend"] == "opencv"


@pytest.mark.parametrize(
    "stub,reason",
    [
        (StubDeepFace(error=ValueError("Face could not be detected")), "no_face"),
        (StubDeepFace(result=[]), "no_face"),
        (StubDeepFace(result=[{"embedding": [0.1] * 512}] * 2), "multiple_faces"),
    ],
)
def test_engine_unusable_cases(stub, reason):
    with pytest.raises(Unusable) as err:
        DeepFaceEngine(stub).embed(decode_image(image_bytes()))
    assert err.value.reason == reason


def test_engine_rejects_unexpected_dimension():
    with pytest.raises(RuntimeError):
        DeepFaceEngine(StubDeepFace(result=[{"embedding": [0.1] * 128}])).embed(
            decode_image(image_bytes())
        )


def test_warm_up_loads_model_and_detector_without_enforcing_a_face():
    stub = StubDeepFace(result=[{"embedding": [0.0] * 512}])
    DeepFaceEngine(stub).warm_up()
    assert stub.calls[0]["detector_backend"] == "opencv"
    assert stub.calls[0]["enforce_detection"] is False


def test_decode_rejects_oversized_dimensions():
    out = BytesIO()
    Image.new("1", (4100, 4100)).save(out, format="PNG")
    with pytest.raises(Unusable):
        decode_image(out.getvalue())

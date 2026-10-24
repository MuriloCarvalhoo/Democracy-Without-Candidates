import base64
from io import BytesIO

import numpy as np
import pytest
from PIL import Image

from dwc_face.app import KEY_HEADER, create_app
from dwc_face.engine import FakeEngine
from dwc_face.settings import Settings

KEY = "k" * 40
AUTH = {KEY_HEADER: KEY}


def image_bytes(seed: int = 1, corner=(10, 20, 30), fmt: str = "PNG", size=(48, 48)) -> bytes:
    """Synthetic noise image (no real person). The corner pixel drives FakeEngine."""
    pixels = np.random.default_rng(seed).integers(0, 256, (size[1], size[0], 3), dtype=np.uint8)
    pixels[0, 0] = corner
    out = BytesIO()
    Image.fromarray(pixels).save(out, format=fmt)
    return out.getvalue()


def b64(data: bytes) -> str:
    return base64.b64encode(data).decode()


def make_settings(**overrides) -> Settings:
    values = dict(
        host="127.0.0.1",
        port=8001,
        key=KEY,
        engine="fake",
        max_distance=0.30,
        max_image_bytes=64 * 1024,
        concurrency=1,
        queue_timeout_secs=0.2,
    )
    values.update(overrides)
    return Settings(**values)


@pytest.fixture
def client():
    return create_app(make_settings(), FakeEngine()).test_client()

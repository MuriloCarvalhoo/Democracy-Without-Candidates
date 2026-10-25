"""Face engines. Images live only in memory: bytes -> PIL -> numpy, never a file.

`DeepFaceEngine` keeps the auth-face engine (DeepFace, Facenet512, OpenCV detector)
but passes a numpy array instead of a temporary JPEG path. `FakeEngine` is a
deterministic stand-in for tests; it proves nothing about real biometrics.
"""

import hashlib
from io import BytesIO

import numpy as np
from PIL import Image

from .trace import trace

DIMENSION = 512
MAX_PIXELS = 4096 * 4096
ACCEPTED_FORMATS = ("JPEG", "PNG")


class Unusable(Exception):
    """The image cannot yield exactly one usable face."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


@trace
def decode_image(data: bytes) -> np.ndarray:
    """Decodes JPEG/PNG bytes into an RGB array, entirely in memory."""
    try:
        with Image.open(BytesIO(data)) as image:
            if image.format not in ACCEPTED_FORMATS:
                raise Unusable("invalid_image")
            width, height = image.size
            if width * height > MAX_PIXELS:
                raise Unusable("invalid_image")
            return np.asarray(image.convert("RGB"))
    except (OSError, ValueError, SyntaxError, Image.DecompressionBombError) as exc:
        raise Unusable("invalid_image") from exc


@trace
class FakeEngine:
    """Test-only engine. Rules on the top-left pixel: black = no face,
    pure red = multiple faces; otherwise an embedding derived from a hash of the
    pixels, so identical images match and different images do not."""

    name = "fake"
    dimension = DIMENSION

    def warm_up(self) -> None:
        pass

    def embed(self, rgb: np.ndarray) -> list[float]:
        corner = tuple(int(c) for c in rgb[0, 0])
        if corner == (0, 0, 0):
            raise Unusable("no_face")
        if corner == (255, 0, 0):
            raise Unusable("multiple_faces")
        digest = hashlib.shake_256(rgb.tobytes() + str(rgb.shape).encode()).digest(
            self.dimension * 2
        )
        raw = np.frombuffer(digest, dtype="<u2").astype(np.float64)
        return [float(x) for x in raw / 32767.5 - 1.0]


@trace
class DeepFaceEngine:
    name = "deepface"
    dimension = DIMENSION
    model_name = "Facenet512"
    detector_backend = "opencv"

    def __init__(self, deepface_module=None):
        if deepface_module is None:
            from deepface import DeepFace as deepface_module  # heavy: TensorFlow
        self._deepface = deepface_module

    def warm_up(self) -> None:
        """Loads (and on first run downloads) model and detector files before serving."""
        blank = np.full((160, 160, 3), 128, dtype=np.uint8)
        self._deepface.represent(
            img_path=blank,
            model_name=self.model_name,
            enforce_detection=False,
            detector_backend=self.detector_backend,
        )

    def embed(self, rgb: np.ndarray) -> list[float]:
        bgr = np.ascontiguousarray(rgb[:, :, ::-1])  # DeepFace expects BGR arrays
        try:
            faces = self._deepface.represent(
                img_path=bgr,
                model_name=self.model_name,
                enforce_detection=True,
                detector_backend=self.detector_backend,
            )
        except ValueError as exc:  # raised by DeepFace when no face is detected
            raise Unusable("no_face") from exc
        if not faces:
            raise Unusable("no_face")
        if len(faces) > 1:
            raise Unusable("multiple_faces")
        embedding = faces[0]["embedding"]
        if len(embedding) != self.dimension:
            raise RuntimeError("unexpected embedding dimension")
        return [float(x) for x in embedding]


@trace
def build(name: str):
    if name == "fake":
        return FakeEngine()
    if name == "deepface":
        return DeepFaceEngine()
    raise ValueError(f"unknown engine {name!r}")

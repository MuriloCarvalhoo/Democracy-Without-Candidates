"""Stateless HTTP API. Nothing is stored; images and embeddings are never logged.

Liveness is NOT validated here: every face response carries
`"liveness": "not_validated"` so the caller cannot mistake a match for proof
of a live person.
"""

import base64
import binascii
import hmac
import ipaddress
import logging
import math
import threading

import numpy as np
from flask import Flask, jsonify, request
from werkzeug.exceptions import HTTPException

from .engine import Unusable, decode_image
from .settings import Settings

KEY_HEADER = "X-DWC-Face-Key"
MAX_CHALLENGE_LEN = 128
LIVENESS = "not_validated"

log = logging.getLogger("dwc_face")


class BadRequest(Exception):
    pass


class Busy(Exception):
    pass


def cosine_distance(a: list[float], b: list[float]) -> float:
    va, vb = np.asarray(a, dtype=np.float64), np.asarray(b, dtype=np.float64)
    na, nb = np.linalg.norm(va), np.linalg.norm(vb)
    if na == 0 or nb == 0:
        raise BadRequest("zero embedding")
    return float(1.0 - np.dot(va, vb) / (na * nb))


def create_app(settings: Settings, engine) -> Flask:
    app = Flask(__name__)
    app.config["MAX_CONTENT_LENGTH"] = settings.max_request_bytes
    slots = threading.BoundedSemaphore(settings.concurrency)
    expected_key = settings.key.encode()

    @app.before_request
    def guard():
        try:
            remote_ok = ipaddress.ip_address(request.remote_addr or "").is_loopback
        except ValueError:
            remote_ok = False
        if not remote_ok:
            return jsonify(error="forbidden"), 403
        supplied = request.headers.get(KEY_HEADER, "").encode()
        if not hmac.compare_digest(supplied, expected_key):
            return jsonify(error="unauthorized"), 401
        return None

    @app.errorhandler(HTTPException)
    def http_error(err):
        return jsonify(error=err.name.lower().replace(" ", "_")), err.code

    @app.errorhandler(BadRequest)
    def bad_request(err):
        return jsonify(error="bad_request", message=str(err)), 400

    @app.errorhandler(Exception)
    def internal(err):
        # Type only: messages could echo request content.
        log.error("engine failure: %s", type(err).__name__)
        return jsonify(error="engine_error"), 500

    def body() -> dict:
        data = request.get_json(silent=True)
        if not isinstance(data, dict):
            raise BadRequest("JSON object required")
        return data

    def challenge_of(data: dict) -> str:
        challenge = data.get("challenge")
        if not isinstance(challenge, str) or not 0 < len(challenge) <= MAX_CHALLENGE_LEN:
            raise BadRequest("challenge required")
        return challenge

    def image_of(data: dict) -> bytes | None:
        """Decoded image bytes, or None when over the size limit."""
        encoded = data.get("image_b64")
        if not isinstance(encoded, str) or not encoded:
            raise BadRequest("image_b64 required")
        try:
            raw = base64.b64decode(encoded, validate=True)
        except (binascii.Error, ValueError) as exc:
            raise BadRequest("invalid base64") from exc
        if len(raw) > settings.max_image_bytes:
            return None
        return raw

    def embedding_of(data: dict) -> list[float]:
        value = data.get("embedding")
        if not isinstance(value, list) or len(value) != engine.dimension:
            raise BadRequest("embedding with wrong dimension")
        if not all(
            isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)
            for x in value
        ):
            raise BadRequest("embedding must contain finite numbers")
        return [float(x) for x in value]

    def run_engine(image: bytes) -> list[float]:
        if not slots.acquire(timeout=settings.queue_timeout_secs):
            raise Busy()
        try:
            return engine.embed(decode_image(image))
        finally:
            slots.release()

    @app.errorhandler(Busy)
    def busy(_err):
        return jsonify(error="busy"), 503

    def unusable(challenge: str, reason: str, **extra):
        return jsonify(
            status="unusable", reason=reason, challenge=challenge, liveness=LIVENESS, **extra
        )

    @app.get("/healthz")
    def healthz():
        return jsonify(status="ok", engine=engine.name, liveness=LIVENESS)

    @app.post("/v1/embed")
    def embed():
        data = body()
        challenge = challenge_of(data)
        image = image_of(data)
        if image is None:
            return jsonify(error="too_large"), 413
        try:
            embedding = run_engine(image)
        except Unusable as err:
            return unusable(challenge, err.reason)
        return jsonify(status="ok", embedding=embedding, challenge=challenge, liveness=LIVENESS)

    @app.post("/v1/compare")
    def compare():
        data = body()
        challenge = challenge_of(data)
        stored = embedding_of(data)
        image = image_of(data)
        if image is None:
            return jsonify(error="too_large"), 413
        try:
            probe = run_engine(image)
        except Unusable as err:
            return unusable(challenge, err.reason, distance=None)
        distance = cosine_distance(stored, probe)
        status = "match" if distance <= settings.max_distance else "no_match"
        return jsonify(
            status=status,
            distance=distance,
            threshold=settings.max_distance,
            challenge=challenge,
            liveness=LIVENESS,
        )

    return app

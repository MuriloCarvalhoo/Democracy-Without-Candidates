"""Startup settings read from the environment. Invalid values fail explicitly."""

import ipaddress
import math
import os
from dataclasses import dataclass

MIN_KEY_LEN = 32


class SettingsError(ValueError):
    pass


@dataclass(frozen=True)
class Settings:
    host: str
    port: int
    key: str
    engine: str
    max_distance: float
    max_image_bytes: int
    concurrency: int
    queue_timeout_secs: float
    # DWC_FACE_DEBUG=1: trace calls (inputs summarized, never images/embeddings).
    debug: bool = False

    @property
    def max_request_bytes(self) -> int:
        # base64 inflates by 4/3; leave room for the JSON envelope and a stored embedding.
        return self.max_image_bytes * 4 // 3 + 64 * 1024


def require_loopback(host: str) -> str:
    """Only literal loopback addresses are accepted (no hostnames, no 0.0.0.0)."""
    try:
        ip = ipaddress.ip_address(host)
    except ValueError as exc:
        raise SettingsError(f"DWC_FACE_HOST must be a literal loopback IP, got {host!r}") from exc
    if not ip.is_loopback:
        raise SettingsError(f"DWC_FACE_HOST must be loopback, got {host!r}")
    return host


def read_key(path: str) -> str:
    try:
        with open(path, encoding="utf-8") as handle:
            key = handle.read().strip()
    except OSError as exc:
        raise SettingsError(f"cannot read DWC_FACE_KEY_FILE: {exc.strerror}") from exc
    if len(key) < MIN_KEY_LEN:
        raise SettingsError(f"shared key must have at least {MIN_KEY_LEN} characters")
    return key


def from_env(env=None) -> Settings:
    env = os.environ if env is None else env

    def number(name, default, cast, minimum):
        raw = env.get(name)
        if raw is None or raw.strip() == "":
            return default
        try:
            value = cast(raw)
        except ValueError as exc:
            raise SettingsError(f"invalid {name}: {raw!r}") from exc
        if not math.isfinite(value) or value < minimum:
            raise SettingsError(f"invalid {name}: {raw!r}")
        return value

    engine = env.get("DWC_FACE_ENGINE", "deepface")
    if engine not in ("deepface", "fake"):
        raise SettingsError(f"invalid DWC_FACE_ENGINE: {engine!r}")
    if engine == "fake" and env.get("DWC_FACE_TEST_MODE") != "1":
        raise SettingsError("DWC_FACE_ENGINE=fake requires DWC_FACE_TEST_MODE=1 (tests only)")

    key_file = env.get("DWC_FACE_KEY_FILE")
    if not key_file:
        raise SettingsError("missing DWC_FACE_KEY_FILE")

    max_distance = number("DWC_FACE_MAX_DISTANCE", 0.30, float, 0.0)
    if max_distance > 2.0:
        raise SettingsError("DWC_FACE_MAX_DISTANCE must be within [0, 2]")
    port = number("DWC_FACE_PORT", 8001, int, 1)
    if port > 65535:
        raise SettingsError("DWC_FACE_PORT must be within 1..65535")

    debug = env.get("DWC_FACE_DEBUG", "").strip()
    if debug not in ("", "0", "1"):
        raise SettingsError(f"invalid DWC_FACE_DEBUG: {debug!r} (use 0 or 1)")

    return Settings(
        host=require_loopback(env.get("DWC_FACE_HOST", "127.0.0.1")),
        port=port,
        key=read_key(key_file),
        engine=engine,
        max_distance=max_distance,
        max_image_bytes=number("DWC_FACE_MAX_IMAGE_BYTES", 2 * 1024 * 1024, int, 1024),
        concurrency=number("DWC_FACE_CONCURRENCY", 2, int, 1),
        queue_timeout_secs=number("DWC_FACE_QUEUE_TIMEOUT_SECS", 5.0, float, 0.0),
        debug=debug == "1",
    )

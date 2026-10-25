"""Opt-in debug tracing (`DWC_FACE_DEBUG=1`): logs class, method and inputs of
decorated callables. Biometric material never reaches the log: bytes, arrays,
embeddings, base64 images and keys are replaced by their type and size.
"""

import functools
import inspect
import logging

log = logging.getLogger("dwc_face.trace")

_enabled = False
MAX_STR = 64
REDACTED_KEYS = {"image_b64", "embedding", "key", "image", "rgb", "bgr", "data"}


def enable(on: bool) -> None:
    """Turns tracing on or off (set once at startup from settings)."""
    global _enabled
    _enabled = bool(on)


def enabled() -> bool:
    return _enabled


def summarize(value, depth: int = 0) -> str:
    """Loggable description of a value; never the content of sensitive data."""
    if value is None or isinstance(value, (bool, int, float)):
        return repr(value)
    if isinstance(value, (bytes, bytearray, memoryview)):
        return f"<bytes len={len(value)}>"
    if type(value).__name__ == "ndarray":
        return f"<ndarray shape={getattr(value, 'shape', '?')}>"
    if isinstance(value, str):
        return repr(value) if len(value) <= MAX_STR else f"<str len={len(value)}>"
    if isinstance(value, (list, tuple)):
        # Lists of numbers are embeddings: size only.
        return f"<{type(value).__name__} len={len(value)}>"
    if isinstance(value, dict):
        if depth > 0:
            return f"<dict keys={len(value)}>"
        items = (
            f"{k!r}: " + (_redacted(v) if str(k) in REDACTED_KEYS else summarize(v, depth + 1))
            for k, v in value.items()
        )
        return "{" + ", ".join(items) + "}"
    return f"<{type(value).__name__}>"


def _redacted(value) -> str:
    size = len(value) if hasattr(value, "__len__") else "?"
    return f"<redacted {type(value).__name__} len={size}>"


def _describe_args(fn, args, kwargs) -> str:
    try:
        bound = inspect.signature(fn).bind_partial(*args, **kwargs)
    except TypeError:
        return ", ".join(summarize(a) for a in args)
    parts = []
    for name, value in bound.arguments.items():
        if name in ("self", "cls"):
            continue
        shown = _redacted(value) if name in REDACTED_KEYS else summarize(value)
        parts.append(f"{name}={shown}")
    return ", ".join(parts)


def trace(target):
    """Decorates a function, or every public method of a class."""
    if inspect.isclass(target):
        for name, member in list(vars(target).items()):
            if not name.startswith("_") and inspect.isfunction(member):
                setattr(target, name, _wrap(member, target.__name__))
        return target
    return _wrap(target, None)


def _wrap(fn, owner):
    qualname = f"{fn.__module__}.{owner + '.' if owner else ''}{fn.__name__}"

    @functools.wraps(fn)
    def wrapper(*args, **kwargs):
        if not _enabled:
            return fn(*args, **kwargs)
        log.debug("call %s(%s)", qualname, _describe_args(fn, args, kwargs))
        try:
            result = fn(*args, **kwargs)
        except Exception as err:
            log.debug("raise %s: %s", qualname, type(err).__name__)
            raise
        log.debug("return %s -> %s", qualname, summarize(result))
        return result

    return wrapper

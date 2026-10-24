import builtins
import tempfile
import threading

import pytest

from dwc_face.app import KEY_HEADER, create_app
from dwc_face.engine import FakeEngine

from conftest import AUTH, b64, image_bytes, make_settings


def embed(client, image: bytes, challenge="c-1", headers=AUTH):
    return client.post("/v1/embed", json={"image_b64": b64(image), "challenge": challenge}, headers=headers)


def compare(client, embedding, image: bytes, challenge="c-2"):
    return client.post(
        "/v1/compare",
        json={"embedding": embedding, "image_b64": b64(image), "challenge": challenge},
        headers=AUTH,
    )


# --- shared secret and loopback -------------------------------------------------


@pytest.mark.parametrize("headers", [{}, {KEY_HEADER: "wrong"}, {KEY_HEADER: ""}])
def test_requests_without_valid_key_are_rejected(client, headers):
    assert client.get("/healthz", headers=headers).status_code == 401
    assert embed(client, image_bytes(), headers=headers).status_code == 401


def test_healthz_reports_liveness_not_validated(client):
    r = client.get("/healthz", headers=AUTH)
    assert r.status_code == 200
    assert r.json == {"status": "ok", "engine": "fake", "liveness": "not_validated"}


@pytest.mark.parametrize("remote", ["10.0.0.7", "192.168.0.10", "0.0.0.0"])
def test_non_loopback_peer_is_rejected_even_with_key(client, remote):
    r = client.get("/healthz", headers=AUTH, environ_base={"REMOTE_ADDR": remote})
    assert r.status_code == 403


# --- input limits ---------------------------------------------------------------


def test_decoded_image_over_limit_is_413():
    app = create_app(make_settings(max_image_bytes=2048), FakeEngine())
    big = image_bytes(size=(64, 64))  # noise PNG, ~12 KiB
    assert len(big) > 2048
    assert embed(app.test_client(), big).status_code == 413


def test_request_body_over_limit_is_413(client):
    huge = "A" * (make_settings().max_request_bytes + 10)
    r = client.post("/v1/embed", json={"image_b64": huge, "challenge": "c"}, headers=AUTH)
    assert r.status_code == 413


@pytest.mark.parametrize(
    "payload",
    [
        {"image_b64": "@@not-base64@@", "challenge": "c"},
        {"image_b64": "", "challenge": "c"},
        {"challenge": "c"},
        {"image_b64": b64(image_bytes())},
        {"image_b64": b64(image_bytes()), "challenge": ""},
        {"image_b64": b64(image_bytes()), "challenge": "x" * 129},
    ],
)
def test_malformed_requests_are_400(client, payload):
    assert client.post("/v1/embed", json=payload, headers=AUTH).status_code == 400


def test_non_json_body_is_400(client):
    r = client.post("/v1/embed", data="nope", headers=AUTH, content_type="text/plain")
    assert r.status_code == 400


# --- unusable images ------------------------------------------------------------


@pytest.mark.parametrize(
    "image,reason",
    [
        (image_bytes(corner=(0, 0, 0)), "no_face"),
        (image_bytes(corner=(255, 0, 0)), "multiple_faces"),
        (b"definitely not an image", "invalid_image"),
        (image_bytes(fmt="GIF"), "invalid_image"),
    ],
)
def test_unusable_images(client, image, reason):
    r = embed(client, image)
    assert r.status_code == 200
    assert r.json == {
        "status": "unusable",
        "reason": reason,
        "challenge": "c-1",
        "liveness": "not_validated",
    }


# --- embed and compare ----------------------------------------------------------


def test_embed_returns_512_floats_and_echoes_challenge(client):
    r = embed(client, image_bytes(), challenge="nonce-xyz")
    assert r.status_code == 200
    assert r.json["status"] == "ok"
    assert r.json["challenge"] == "nonce-xyz"
    assert r.json["liveness"] == "not_validated"
    assert len(r.json["embedding"]) == 512
    assert all(isinstance(x, float) for x in r.json["embedding"])


def test_compare_match_no_match_and_unusable(client):
    enrolled = embed(client, image_bytes(seed=1)).json["embedding"]

    same = compare(client, enrolled, image_bytes(seed=1)).json
    assert same["status"] == "match"
    assert same["distance"] == pytest.approx(0.0, abs=1e-9)
    assert same["threshold"] == 0.30
    assert same["liveness"] == "not_validated"

    other = compare(client, enrolled, image_bytes(seed=2)).json
    assert other["status"] == "no_match"
    assert other["distance"] > 0.30

    none = compare(client, enrolled, image_bytes(corner=(0, 0, 0))).json
    assert none["status"] == "unusable"
    assert none["distance"] is None


def test_compare_threshold_is_configurable(client):
    enrolled = embed(client, image_bytes(seed=1)).json["embedding"]
    lax = create_app(make_settings(max_distance=2.0), FakeEngine()).test_client()
    assert compare(lax, enrolled, image_bytes(seed=2)).json["status"] == "match"


@pytest.mark.parametrize(
    "embedding", [[0.1] * 10, [0.0] * 512, ["x"] * 512, [True] * 512, "not-a-list", None]
)
def test_compare_rejects_invalid_embeddings(client, embedding):
    assert compare(client, embedding, image_bytes()).status_code == 400


# --- concurrency ----------------------------------------------------------------


class BlockingEngine(FakeEngine):
    def __init__(self):
        self.entered = threading.Event()
        self.release = threading.Event()

    def embed(self, rgb):
        self.entered.set()
        self.release.wait(5)
        return super().embed(rgb)


def test_concurrency_is_limited():
    engine = BlockingEngine()
    app = create_app(make_settings(concurrency=1, queue_timeout_secs=0.05), engine)
    results = {}

    def first():
        results["first"] = embed(app.test_client(), image_bytes()).status_code

    worker = threading.Thread(target=first)
    worker.start()
    assert engine.entered.wait(5)
    assert embed(app.test_client(), image_bytes()).status_code == 503
    engine.release.set()
    worker.join(5)
    assert results["first"] == 200


# --- no files -------------------------------------------------------------------


def test_requests_write_no_files(client, tmp_path, monkeypatch):
    def forbidden(*_args, **_kwargs):
        raise AssertionError("temporary file created")

    for name in ("NamedTemporaryFile", "TemporaryFile", "SpooledTemporaryFile", "mkstemp", "mkdtemp"):
        monkeypatch.setattr(tempfile, name, forbidden)
    real_open = builtins.open

    def read_only_open(file, mode="r", *args, **kwargs):
        if any(flag in mode for flag in "wax+"):
            raise AssertionError(f"file opened for writing: {file}")
        return real_open(file, mode, *args, **kwargs)

    monkeypatch.setattr(builtins, "open", read_only_open)
    scratch = tmp_path / "tmp"
    scratch.mkdir()
    monkeypatch.setattr(tempfile, "tempdir", str(scratch))
    monkeypatch.chdir(tmp_path)

    enrolled = embed(client, image_bytes(fmt="JPEG")).json["embedding"]
    assert compare(client, enrolled, image_bytes(fmt="JPEG")).json["status"] == "match"
    assert embed(client, image_bytes(corner=(0, 0, 0))).json["status"] == "unusable"
    assert sorted(p.name for p in tmp_path.iterdir()) == ["tmp"]
    assert list(scratch.iterdir()) == []

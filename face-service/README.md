# face-service

Private, stateless face engine for the Rust app. It reuses only the engine of
[auth-face `d873629`](https://github.com/MuriloCarvalhoo/auth-face/blob/d873629f6f84fd858162e3a73435002811d903cc/artifacts/face-service/main.py)
(DeepFace, Facenet512, OpenCV detector). Rust owns accounts, templates, sessions and
authorization; this service only extracts and compares embeddings.

## Run

```sh
just face-setup                                   # venv + HTTP layer + pytest
face-service/.venv/bin/pip install -r face-service/requirements-engine.txt   # TensorFlow/DeepFace
python3 -c "import secrets; print(secrets.token_hex(32))" > /etc/dwc/face.key
cd face-service && DWC_FACE_KEY_FILE=/etc/dwc/face.key .venv/bin/python main.py
```

The first start downloads model weights to `~/.deepface/weights`. The Rust app uses
`DWC_IDENTITY_PROVIDER=face-service`, `DWC_FACE_SERVICE_URL=http://127.0.0.1:8001`
and `DWC_FACE_SERVICE_KEY_FILE` (same file). See `dwc-face.service` for systemd.

## Test

`just face-test` runs pytest with the deterministic fake engine (no TensorFlow).
The Rust client has mock-server tests in `cargo test` and a manual contract test:
`DWC_FACE_IT_URL=… DWC_FACE_IT_KEY_FILE=… DWC_FACE_IT_ENGINE=fake|deepface cargo test --features ssr live_service_contract -- --ignored`.

## Settings

| Variable | Default | Notes |
| --- | --- | --- |
| `DWC_FACE_KEY_FILE` | required | Shared secret (≥ 32 chars) sent as `X-DWC-Face-Key` |
| `DWC_FACE_HOST` | `127.0.0.1` | Literal loopback IP only; anything else refuses to start |
| `DWC_FACE_PORT` | `8001` | |
| `DWC_FACE_MAX_DISTANCE` | `0.30` | Cosine distance; equals the inherited 0.70 similarity |
| `DWC_FACE_MAX_IMAGE_BYTES` | `2097152` | Decoded image limit (JPEG/PNG, ≤ 4096×4096 px) |
| `DWC_FACE_CONCURRENCY` | `2` | Parallel engine runs; extra requests wait, then get 503 |
| `DWC_FACE_QUEUE_TIMEOUT_SECS` | `5` | Wait for a free engine slot |
| `DWC_FACE_ENGINE` | `deepface` | `fake` only with `DWC_FACE_TEST_MODE=1` |
| `DWC_FACE_DEBUG` | `0` | `1` logs each traced call (module, class, method, inputs); bytes, arrays, embeddings, base64 images and the key appear only as type and size |

## API

All routes require the key header and a loopback peer. Nothing is stored or logged
(`DWC_FACE_DEBUG` logs only sizes);
images are decoded in memory (no temporary files).

- `GET /healthz` → `{status, engine, liveness}`
- `POST /v1/embed` `{image_b64, challenge}` → `{status: ok|unusable, embedding?, reason?, challenge, liveness}`
- `POST /v1/compare` `{embedding, image_b64, challenge}` → `{status: match|no_match|unusable, distance, threshold?, challenge, liveness}`

Zero or several faces, or an undecodable image, return `unusable`. Errors: 400
malformed, 401 key, 403 non-loopback peer, 413 too large, 503 busy, 500 engine.

## Not validated

- **Liveness**: not checked server-side; every response says `"liveness": "not_validated"`.
  The browser blink check from auth-face is not evidence. The challenge is only echoed.
- **Matching accuracy**: the 0.30/0.70 threshold is inherited, not calibrated; false
  match/non-match rates are unknown.
- **Presentation attacks** (photos, screens, masks) and demographic bias: not tested.
- One-person uniqueness: a face match does not prove it.

Real identity use stays blocked until these have evidence.

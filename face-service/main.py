"""Entry point: validate settings, load the engine, serve on loopback with waitress."""

import logging
import sys

from waitress import serve

from dwc_face import engine as engines
from dwc_face import trace
from dwc_face.app import create_app
from dwc_face.settings import SettingsError, from_env


def main() -> int:
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s %(message)s")
    log = logging.getLogger("dwc_face")
    try:
        settings = from_env()
    except SettingsError as err:
        log.error("configuration: %s", err)
        return 2
    if settings.debug:
        trace.enable(True)
        logging.getLogger("dwc_face").setLevel(logging.DEBUG)
        log.warning("DWC_FACE_DEBUG is on: calls are traced (inputs summarized, no images)")
    engine = engines.build(settings.engine)
    engine.warm_up()
    log.warning("liveness is not validated server-side; matches are not proof of a live person")
    log.info(
        "listening on %s:%d engine=%s concurrency=%d",
        settings.host,
        settings.port,
        engine.name,
        settings.concurrency,
    )
    serve(
        create_app(settings, engine),
        host=settings.host,
        port=settings.port,
        threads=settings.concurrency + 2,
        max_request_body_size=settings.max_request_bytes,
        ident="dwc-face",
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Serve one fixture directory on an ephemeral loopback port."""

from __future__ import annotations

import sys
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, _format: str, *_args: object) -> None:
        pass


def main() -> None:
    root = Path(sys.argv[1]).resolve()
    port_file = Path(sys.argv[2]).resolve()
    handler = partial(QuietHandler, directory=str(root))
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    port_file.write_text(str(server.server_port), encoding="ascii")
    server.serve_forever()


if __name__ == "__main__":
    main()

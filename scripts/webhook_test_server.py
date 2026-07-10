#!/usr/bin/env python3
"""Local webhook sink for NapCatQQ Desktop offline-notify verification.

Usage:
  python scripts/webhook_test_server.py
  python scripts/webhook_test_server.py --port 9876 --secret my-token

Desktop settings:
  URL:    http://127.0.0.1:9876/webhook
  Method: POST
  Secret: same as --secret (optional; sent as Authorization: Bearer <secret>)
  Body:   any preset / custom JSON with {nickname} {uin} {event} {time}

Press Ctrl+C to stop.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import parse_qs, urlparse


def _now() -> str:
    return datetime.now().strftime("%Y-%m-%d %H:%M:%S")


def _pretty(data: Any) -> str:
    try:
        return json.dumps(data, ensure_ascii=False, indent=2)
    except TypeError:
        return str(data)


class WebhookHandler(BaseHTTPRequestHandler):
    server_version = "NcdWebhookTest/1.0"
    expected_secret: str = ""
    require_secret: bool = False

    def log_message(self, fmt: str, *args: Any) -> None:
        # quieter default access log; we print structured dumps ourselves
        sys.stderr.write(f"[{_now()}] {self.address_string()} {fmt % args}\n")

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0:
            return b""
        return self.rfile.read(length)

    def _auth_ok(self) -> bool:
        if not self.require_secret and not self.expected_secret:
            return True
        auth = self.headers.get("Authorization", "")
        expected = f"Bearer {self.expected_secret}"
        if auth == expected:
            return True
        # also accept raw secret for convenience
        if self.expected_secret and auth == self.expected_secret:
            return True
        return False

    def _dump(self, body: bytes) -> None:
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)
        print("\n" + "=" * 72)
        print(f"[{_now()}] {self.command} {self.path}")
        print(f"client : {self.client_address[0]}:{self.client_address[1]}")
        print(f"path   : {parsed.path}")
        if query:
            print("query  :")
            print(_pretty({k: v if len(v) > 1 else v[0] for k, v in query.items()}))
        print("headers:")
        for key, value in self.headers.items():
            # keep secret visible for local debug (this is a test sink)
            print(f"  {key}: {value}")
        if body:
            text = body.decode("utf-8", errors="replace")
            print("body   :")
            try:
                print(_pretty(json.loads(text)))
            except json.JSONDecodeError:
                print(text)
        else:
            print("body   : <empty>")
        print("=" * 72 + "\n", flush=True)

    def _reply(self, code: int, payload: dict[str, Any]) -> None:
        raw = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self) -> None:  # noqa: N802
        body = self._read_body()
        self._dump(body)
        if not self._auth_ok():
            self._reply(401, {"ok": False, "error": "unauthorized"})
            return
        if urlparse(self.path).path in ("/", "/health"):
            self._reply(200, {"ok": True, "service": "ncd-webhook-test", "hint": "POST /webhook"})
            return
        self._reply(200, {"ok": True, "method": "GET", "received": True})

    def do_POST(self) -> None:  # noqa: N802
        body = self._read_body()
        self._dump(body)
        if not self._auth_ok():
            self._reply(401, {"ok": False, "error": "unauthorized"})
            return
        path = urlparse(self.path).path
        if path not in ("/webhook", "/", "/notify", "/hook"):
            self._reply(404, {"ok": False, "error": f"unknown path: {path}"})
            return
        self._reply(200, {"ok": True, "method": "POST", "received": True})

    def do_PUT(self) -> None:  # noqa: N802
        # Desktop currently uses POST/GET only; accept PUT for manual curl checks
        self.do_POST()


def main() -> int:
    parser = argparse.ArgumentParser(description="Local webhook sink for Desktop offline notify")
    parser.add_argument("--host", default="127.0.0.1", help="bind host (default 127.0.0.1)")
    parser.add_argument("--port", type=int, default=9876, help="bind port (default 9876)")
    parser.add_argument(
        "--secret",
        default="",
        help="optional Bearer secret; if set, Authorization must match",
    )
    parser.add_argument(
        "--require-secret",
        action="store_true",
        help="reject requests without matching Authorization even if --secret empty is not used",
    )
    args = parser.parse_args()

    WebhookHandler.expected_secret = args.secret
    WebhookHandler.require_secret = bool(args.secret) or args.require_secret

    server = ThreadingHTTPServer((args.host, args.port), WebhookHandler)
    print(f"[{_now()}] webhook test server listening on http://{args.host}:{args.port}")
    print(f"  POST URL for Desktop: http://{args.host}:{args.port}/webhook")
    if args.secret:
        print(f"  expected Authorization: Bearer {args.secret}")
    else:
        print("  secret: (none) — any Authorization accepted")
    print("  health: GET /health")
    print("Ctrl+C to stop.\n", flush=True)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print(f"\n[{_now()}] stopped.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

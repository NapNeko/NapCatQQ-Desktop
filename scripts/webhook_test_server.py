#!/usr/bin/env python3
"""Local webhook sink for NapCatQQ Desktop offline-notify verification.

Usage:
  uv run scripts/webhook_test_server.py
  uv run scripts/webhook_test_server.py --port 9876 --secret my-token
  uv run scripts/webhook_test_server.py --host 0.0.0.0 --port 9876

Desktop 本机通知（Desktop 进程自己发 Webhook）:
  URL:    http://127.0.0.1:9876/webhook
  Method: POST
  Secret: same as --secret（可选；Authorization: Bearer <secret>）

远端 ncd-watch 联调（重要）:
  notify.json 里的 URL 在「远端机器」上解析。
  填 http://127.0.0.1:9876/webhook 会打到服务器自己，不是 Windows。

  推荐：本机监听 127.0.0.1 + SSH 反向隧道（无需开防火墙）：
    # 终端 A
    uv run scripts/webhook_test_server.py --host 127.0.0.1 --port 9876 --secret ncd-test-secret
    # 终端 B（密钥路径按 server 档案）
    ssh -i "<ssh_key>" -N -R 9876:127.0.0.1:9876 <user>@<remote>
    # Desktop Webhook URL 仍可写 http://127.0.0.1:9876/webhook，同步后远端经隧道打到本机。

  备选：--host 0.0.0.0 + 防火墙放行 + URL 改电脑局域网 IP。
  Hyper-V/NAT 远端常访问不到宿主机 WLAN IP，优先反向隧道。

  ncd-watch 默认 desktop_present 在线只探不报：完全退出 Desktop（或等 present TTL
  ~90s）后再制造 online→offline；冷启动时已 offline 不刷历史告警。

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
    print(f"  POST URL for Desktop / ncd-watch: http://127.0.0.1:{args.port}/webhook")
    if args.host in ("0.0.0.0", "::"):
        print("  bound all interfaces — remote may use your LAN IP if firewall allows")
    else:
        print("  bound loopback only — for remote ncd-watch use SSH reverse tunnel:")
        print(f"    ssh -N -R {args.port}:127.0.0.1:{args.port} <user>@<remote>")
    if args.secret:
        print(f"  expected Authorization: Bearer {args.secret}")
    else:
        print("  secret: (none) — any Authorization accepted")
    print("  health: GET /health")
    print("  note: ncd-watch skips webhooks while desktop_present is fresh (Desktop online)")
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

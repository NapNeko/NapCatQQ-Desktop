# -*- coding: utf-8 -*-
"""
此测试脚本会启动一个简单的 HTTP 服务器来接收 WebHook 请求。

使用方法:
1. 运行脚本：`python webhook.py`
2. 服务器将在端口 8000 上监听 WebHook 请求。
3. 发送 POST 请求到 `http://localhost:8000/`，请求体应为 JSON 格式。
4. 服务器将打印接收到的请求数据并返回一个简单的 JSON 响应。

注意事项:
- 确保端口 8000 未被其他服务占用。
- 该脚本仅用于测试目的，不适合生产环境。
"""

# 标准库导入
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse


class WebhookHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        # 获取内容长度
        content_length = int(self.headers.get("Content-Length", 0))

        # 读取请求体
        post_data = self.rfile.read(content_length)

        try:
            # 解析 JSON 数据
            data = json.loads(post_data.decode("utf-8"))
        except json.JSONDecodeError:
            data = {"error": "Invalid JSON"}

        # 获取请求头中的密钥（如果有）
        auth_header = self.headers.get("Authorization", "")
        api_key = self.headers.get("X-API-Key", "")

        # 打印接收到的信息
        print("=" * 50)
        print("收到 WebHook 请求:")
        print(f"路径: {self.path}")
        print(f"认证头: {auth_header}")
        print(f"API密钥: {api_key}")
        print("请求体数据:")
        print(json.dumps(data, indent=2, ensure_ascii=False))
        print("=" * 50)

        # 返回成功响应
        self.send_response(200)
        self.send_header("Content-type", "application/json")
        self.end_headers()

        response = {"status": "success", "message": "WebHook received successfully", "received_data": data}

        self.wfile.write(json.dumps(response).encode("utf-8"))


def run_server(port=8000):
    server_address = ("", port)
    httpd = HTTPServer(server_address, WebhookHandler)
    print(f"WebHook 接收服务器启动在端口 {port}...")
    print("按 Ctrl+C 停止服务器")

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\n服务器已停止")


if __name__ == "__main__":
    run_server()

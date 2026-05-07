# -*- coding: utf-8 -*-
"""[`sanitize_text_for_export`](src/core/logging/crash_bundle.py) host /
username / tunnel label 脱敏 单测 (P5 安全收尾 F3.2).

背景: TRCE 级别日志会原样写主机名 / 用户名到 ``app.log``, 例如:

    执行远程命令: host=ac.rainplay.cn, timeout=15.0, ...
    SSH 连接已建立: host=ac.rainplay.cn, username=root
    SSH 隧道已建立: label=ac.rainplay.cn->127.0.0.1:6099

提交诊断包到上游 issue 时, 这些字段会暴露用户的服务器地址 + SSH 入口用户名,
方便定向 SSH 暴力破解 / 端口扫描. 本文件覆盖三条窄正则:

- ``host=`` / ``hostname=`` -> 仅保留首字符 + 顶级域 (``ac.rainplay.cn`` -> ``a***.cn``)
- ``username=`` -> 仅保留首字符 (``root`` -> ``r***``)
- ``label=<host>-><...>`` -> 把箭头左边的 host 部分脱敏

替换语义: 仅触碰捕获组里的 host/username 段, 前后字面量原样保留.
"""
from __future__ import annotations

# 项目内模块导入
from src.core.logging.crash_bundle import sanitize_text_for_export


# ==================== mask_host 行为 ====================
def test_mask_host_for_domain() -> None:
    """``mask_host`` 对域名保留首字符 + 顶级域."""
    from src.core.logging.crash_bundle import mask_host

    assert mask_host("ac.rainplay.cn") == "a***.cn"
    assert mask_host("server.example.com") == "s***.com"
    assert mask_host("a.b") == "a***.b"


def test_mask_host_for_ip() -> None:
    """``mask_host`` 对 IPv4 保留首字符 + 末段."""
    from src.core.logging.crash_bundle import mask_host

    # 10.0.0.5 -> 1***.5 (首字符 + 最后一段)
    assert mask_host("10.0.0.5") == "1***.5"
    assert mask_host("192.168.1.100") == "1***.100"


def test_mask_host_for_short_name_without_dot() -> None:
    """无点的主机名仅保留首字符."""
    from src.core.logging.crash_bundle import mask_host

    assert mask_host("myserver") == "m***"


def test_mask_host_for_empty() -> None:
    from src.core.logging.crash_bundle import mask_host

    assert mask_host("") == "<empty-host>"
    assert mask_host(None) == "<empty-host>"


# ==================== host=/hostname= 命中 ====================
def test_host_kv_in_log_is_masked() -> None:
    text = "执行远程命令: host=ac.rainplay.cn, timeout=15.0"
    out = sanitize_text_for_export(text)
    assert "ac.rainplay.cn" not in out
    assert "host=a***.cn" in out


def test_hostname_kv_in_log_is_masked() -> None:
    text = "已解析远端 hostname=server.example.com"
    out = sanitize_text_for_export(text)
    assert "server.example.com" not in out
    assert "hostname=s***.com" in out


def test_host_kv_with_quotes_is_masked() -> None:
    text = 'config "host"="ac.rainplay.cn"'
    out = sanitize_text_for_export(text)
    assert "ac.rainplay.cn" not in out
    assert "a***.cn" in out


def test_host_kv_with_ip_is_masked() -> None:
    text = "host=10.0.0.5, port=22"
    out = sanitize_text_for_export(text)
    assert "10.0.0.5" not in out
    assert "host=1***.5" in out


# ==================== username= 命中 ====================
def test_username_kv_is_masked() -> None:
    text = "SSH 连接已建立: host=ac.rainplay.cn, username=root"
    out = sanitize_text_for_export(text)
    assert "root" not in out
    assert "username=r***" in out


def test_username_kv_with_long_name_is_masked() -> None:
    text = "username=alice_dev"
    out = sanitize_text_for_export(text)
    assert "alice_dev" not in out
    assert "username=a***" in out


# ==================== label= (tunnel) 命中 ====================
def test_tunnel_label_with_host_is_masked() -> None:
    text = "SSH 隧道已建立: label=ac.rainplay.cn->127.0.0.1:6099, local=..."
    out = sanitize_text_for_export(text)
    assert "ac.rainplay.cn" not in out
    assert "label=a***.cn->127.0.0.1:6099" in out


def test_tunnel_label_with_ip_host_is_masked() -> None:
    text = "label=10.0.0.5->127.0.0.1:6099"
    out = sanitize_text_for_export(text)
    assert "10.0.0.5" not in out
    assert "label=1***.5->127.0.0.1:6099" in out


# ==================== 综合场景 ====================
def test_combined_log_redacts_host_and_username_together() -> None:
    text = "SSH 连接已建立: host=ac.rainplay.cn, username=root, port=22"
    out = sanitize_text_for_export(text)
    assert "ac.rainplay.cn" not in out
    assert "root" not in out
    assert "host=a***.cn" in out
    assert "username=r***" in out


def test_existing_qqid_pattern_still_works() -> None:
    """F3.1 的 QQID 脱敏不应被 F3.2 改动破坏."""
    text = "执行命令: qq --no-sandbox -q 3217681217 on host=ac.rainplay.cn"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "ac.rainplay.cn" not in out
    assert "***1217" in out
    assert "a***.cn" in out


def test_existing_secret_pattern_still_works() -> None:
    """``token=...`` 等历史命中不应被 F3.2 改动破坏."""
    text = 'host=ac.rainplay.cn, token=topsecret123'
    out = sanitize_text_for_export(text)
    assert "topsecret123" not in out
    assert "ac.rainplay.cn" not in out


# ==================== 不应误伤 ====================
def test_unrelated_kv_with_host_in_value_is_not_touched() -> None:
    """``foo=bar`` 这种非 host/username/label 键不应被处理."""
    text = "config_path=/etc/napcat.conf"
    out = sanitize_text_for_export(text)
    assert "/etc/napcat.conf" in out


def test_word_containing_host_is_not_touched() -> None:
    """``hosting=true`` 等带 host 子串的不应误伤."""
    text = "hosting=true"
    out = sanitize_text_for_export(text)
    # 现有正则要求 ``host=`` 后立即接值, ``hosting=`` 不应命中
    assert "hosting=true" in out


def test_url_pattern_still_redacts_host_inside_url() -> None:
    """完整 URL 走 _URL_PATTERN, host 子句不会重复处理."""
    text = "fetched from https://ac.rainplay.cn/api"
    out = sanitize_text_for_export(text)
    # URL 整体被替换为 <redacted-url>
    assert "ac.rainplay.cn" not in out
    assert "<redacted-url>" in out

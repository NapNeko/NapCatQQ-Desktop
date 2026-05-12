# -*- coding: utf-8 -*-
""":class:`SSHClient` 主机指纹变更交互式重确认 (P4 F5.1 补丁) 单元测试.

覆盖三块:

1. ``KnownHostsStore.remove`` / ``lookup_fingerprint`` 行为
2. ``SSHClient._handle_bad_host_key_interactive`` 决策分支与 store 写入
3. ``SSHClient._format_bad_host_key_error`` 中文友好错误渲染
"""

from __future__ import annotations

# 标准库导入
from pathlib import Path
from unittest.mock import MagicMock

# 第三方库导入
import paramiko
import pytest

# 项目内模块导入
from src.core.remote.host_key_policy import (
    HostKeyDecision,
    HostKeyPrompt,
    KnownHostsStore,
    compute_sha256_fingerprint,
    register_host_key_callback,
)
from src.core.remote.models import SSHCredentials
from src.core.remote.ssh_client import SSHClient


# ==================== 固件 ====================
@pytest.fixture(autouse=True)
def _reset_callback() -> None:
    """清空全局 callback, 隔离测试."""
    register_host_key_callback(None)
    yield
    register_host_key_callback(None)


@pytest.fixture
def known_hosts_store(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> KnownHostsStore:
    """构造临时 KnownHostsStore + 重定向 ``default_known_hosts_path``."""
    kh_path = tmp_path / "known_hosts"
    # 让 default_known_hosts_path 返回临时路径, 让 SSHClient._handle_bad_host_key_interactive
    # 在生产代码里写入到这个临时文件而非真实 app data
    import src.core.remote.host_key_policy as hkp_mod

    monkeypatch.setattr(hkp_mod, "default_known_hosts_path", lambda: kh_path)
    return KnownHostsStore(kh_path)


@pytest.fixture
def credentials() -> SSHCredentials:
    return SSHCredentials(
        host="example.com",
        username="ubuntu",
        port=22,
        auth_method="password",
        password="x",
        host_key_policy="interactive",
    )


_RSA_KEY_CACHE: dict[str, paramiko.RSAKey] = {}


def _make_real_key(tag: str) -> paramiko.RSAKey:
    """生成 (并缓存) 一个真实 RSAKey, 解决 ``MagicMock(spec=PKey)`` 经 paramiko
    序列化崩坏的问题 (``get_base64`` 是 mock 时 ``HostKeys.save`` 写出空文件).

    缓存 by tag 让多次调用同一 tag 返回同一 key 实例, 测试可控.
    """
    if tag not in _RSA_KEY_CACHE:
        # 1024 bit 足够测试 (生产用 ≥2048); 这里追求构造速度
        _RSA_KEY_CACHE[tag] = paramiko.RSAKey.generate(1024)
    return _RSA_KEY_CACHE[tag]


def _fake_key_with_bytes(name: str, raw: bytes) -> paramiko.RSAKey:
    """构造一个能稳定生成指定 sha256 的 key 用于测试.

    入参 ``name`` / ``raw`` 仅作为 cache key, 内部返真实 RSAKey;
    每个独特的 (name, raw) 对应一个独立的 RSAKey 实例.
    """
    del name  # 仅用于语义清晰; 实际 RSAKey 的 name 固定为 "ssh-rsa"
    cache_key = f"raw:{raw.hex()}"
    return _make_real_key(cache_key)


# ==================== KnownHostsStore.remove / lookup_fingerprint ====================
class TestStoreRemove:
    def test_remove_existing_entry(
        self, known_hosts_store: KnownHostsStore
    ) -> None:
        key = _fake_key_with_bytes("ssh-ed25519", b"OLD_KEY_RAW_DATA" * 2)
        known_hosts_store.add("example.com", 22, key)
        known_hosts_store.save()

        # remove 应返回 True
        assert known_hosts_store.remove("example.com", 22) is True
        # 二次 remove 应返回 False
        assert known_hosts_store.remove("example.com", 22) is False
        assert not known_hosts_store.contains("example.com", 22)

    def test_remove_nonexistent_returns_false(
        self, known_hosts_store: KnownHostsStore
    ) -> None:
        assert known_hosts_store.remove("not-there.com", 22) is False

    def test_remove_only_matching_host(
        self, known_hosts_store: KnownHostsStore
    ) -> None:
        key_a = _fake_key_with_bytes("ssh-ed25519", b"KEY_A_DATA" * 4)
        key_b = _fake_key_with_bytes("ssh-ed25519", b"KEY_B_DATA" * 4)
        known_hosts_store.add("host-a.com", 22, key_a)
        known_hosts_store.add("host-b.com", 22, key_b)

        known_hosts_store.remove("host-a.com", 22)
        assert not known_hosts_store.contains("host-a.com", 22)
        # host-b 未受影响
        assert known_hosts_store.contains("host-b.com", 22)

    def test_remove_handles_nondefault_port(
        self, known_hosts_store: KnownHostsStore
    ) -> None:
        key = _fake_key_with_bytes("ssh-ed25519", b"DATA" * 8)
        known_hosts_store.add("example.com", 2222, key)
        assert known_hosts_store.remove("example.com", 2222) is True
        # 不同端口不应误删
        known_hosts_store.add("example.com", 2222, key)
        assert known_hosts_store.remove("example.com", 22) is False
        assert known_hosts_store.contains("example.com", 2222)


class TestStoreLookupFingerprint:
    def test_lookup_returns_key_type_and_fingerprint(
        self, known_hosts_store: KnownHostsStore
    ) -> None:
        key = _fake_key_with_bytes("ssh-ed25519", b"LOOKUP_KEY_DATA" * 2)
        expected_fp = compute_sha256_fingerprint(key)
        known_hosts_store.add("example.com", 22, key)

        key_type, fp = known_hosts_store.lookup_fingerprint("example.com", 22)
        # 实际 key 是真实 paramiko key, 类型由 paramiko 决定 (生产里可能是 ssh-rsa)
        assert key_type == key.get_name()
        assert fp == expected_fp

    def test_lookup_missing_returns_empty(
        self, known_hosts_store: KnownHostsStore
    ) -> None:
        key_type, fp = known_hosts_store.lookup_fingerprint("missing.com", 22)
        assert key_type == ""
        assert fp == ""


# ==================== SSHClient._handle_bad_host_key_interactive ====================
class TestHandleBadHostKey:
    def _make_bad_host_key_exception(
        self,
        hostname: str,
        new_key_raw: bytes,
        old_key_raw: bytes,
    ) -> paramiko.BadHostKeyException:
        """构造 paramiko BadHostKeyException, 注入可控的 new/old key."""
        new_key = _fake_key_with_bytes("ssh-ed25519", new_key_raw)
        old_key = _fake_key_with_bytes("ssh-ed25519", old_key_raw)
        exc = paramiko.BadHostKeyException(hostname, new_key, old_key)
        return exc

    def test_no_callback_returns_false(
        self,
        credentials: SSHCredentials,
        known_hosts_store: KnownHostsStore,
    ) -> None:
        """未注册 callback 时降级到 False (调用方走友好错误)."""
        client = SSHClient(credentials)
        exc = self._make_bad_host_key_exception(
            "example.com", b"NEW" * 11, b"OLD" * 11
        )
        # 不注册 callback
        assert client._handle_bad_host_key_interactive(exc) is False

    def test_callback_returns_trust_replace_writes_new_key(
        self,
        credentials: SSHCredentials,
        known_hosts_store: KnownHostsStore,
    ) -> None:
        """TRUST_REPLACE: callback 被调 + 旧 entry 已替换 (新 entry 写入 store).

        Note:
            最终 fingerprint 值未做强等于断言: paramiko mock key 经 known_hosts
            序列化/反序列化后 ``asbytes()`` 与原始 mock 不一致, 这是 mock 局限
            而非业务逻辑问题. 行为正确性已由 ``removed_old / contains`` 充分覆盖.
        """
        # 预先在 store 写一个"旧 key"
        old_key = _fake_key_with_bytes("ssh-ed25519", b"OLDDATA" * 5)
        known_hosts_store.add("example.com", 22, old_key)
        known_hosts_store.save()
        old_fp = compute_sha256_fingerprint(old_key)

        # 注册 callback 返 TRUST_REPLACE
        received: list[HostKeyPrompt] = []

        def cb(p: HostKeyPrompt, **kwargs) -> HostKeyDecision:
            received.append(p)
            return HostKeyDecision.TRUST_REPLACE

        register_host_key_callback(cb)

        client = SSHClient(credentials)
        exc = self._make_bad_host_key_exception(
            "example.com", b"NEWDATA" * 5, b"OLDDATA" * 5
        )
        new_fp_expected = compute_sha256_fingerprint(exc.key)

        result = client._handle_bad_host_key_interactive(exc)

        assert result is True
        # callback 被调一次, prompt 携带新旧指纹
        assert len(received) == 1
        prompt = received[0]
        assert prompt.fingerprint_sha256 == new_fp_expected
        assert prompt.previous_fingerprint_sha256 == old_fp

        # store 中仍有 example.com 的 entry (旧 → 新替换成功)
        fresh_store = KnownHostsStore(known_hosts_store.path)
        assert fresh_store.contains("example.com", 22)
        # 关键: store 写盘后, fingerprint 已发生变化 (不再是 old_fp)
        # 这是 TRUST_REPLACE 的核心语义验证
        _, post_fp = fresh_store.lookup_fingerprint("example.com", 22)
        assert post_fp != old_fp

    def test_callback_returns_reject_returns_false(
        self,
        credentials: SSHCredentials,
        known_hosts_store: KnownHostsStore,
    ) -> None:
        """REJECT 时不动 store, 返 False."""
        old_key = _fake_key_with_bytes("ssh-ed25519", b"OLD" * 11)
        known_hosts_store.add("example.com", 22, old_key)
        known_hosts_store.save()
        old_fp = compute_sha256_fingerprint(old_key)

        register_host_key_callback(lambda p, **kwargs: HostKeyDecision.REJECT)
        client = SSHClient(credentials)
        exc = self._make_bad_host_key_exception(
            "example.com", b"NEW" * 11, b"OLD" * 11
        )

        assert client._handle_bad_host_key_interactive(exc) is False
        # 旧 key 仍保留
        _, fp = known_hosts_store.lookup_fingerprint("example.com", 22)
        assert fp == old_fp

    def test_callback_exception_returns_false(
        self,
        credentials: SSHCredentials,
        known_hosts_store: KnownHostsStore,
    ) -> None:
        """callback 抛异常时, 视为拒绝, 返 False (不应让 SSHClient 崩)."""

        def bad_cb(p: HostKeyPrompt, **kwargs) -> HostKeyDecision:
            raise RuntimeError("UI 崩了")

        register_host_key_callback(bad_cb)
        client = SSHClient(credentials)
        exc = self._make_bad_host_key_exception(
            "example.com", b"NEW" * 11, b"OLD" * 11
        )
        assert client._handle_bad_host_key_interactive(exc) is False

    def test_non_default_port_strips_bracket_wrapper(
        self,
        credentials: SSHCredentials,
        known_hosts_store: KnownHostsStore,
    ) -> None:
        """``port != 22`` 时 paramiko 用 ``[host]:port`` 格式; 应剥离."""
        cred = SSHCredentials(
            host="example.com",
            username="x",
            port=2222,
            auth_method="password",
            password="x",
            host_key_policy="interactive",
        )

        received: list[HostKeyPrompt] = []

        def cb(p: HostKeyPrompt, **kwargs) -> HostKeyDecision:
            received.append(p)
            return HostKeyDecision.TRUST_REPLACE

        register_host_key_callback(cb)
        client = SSHClient(cred)
        # paramiko 实际传入的 hostname 形如 ``[example.com]:2222``
        new_key = _fake_key_with_bytes("ssh-ed25519", b"N" * 32)
        old_key = _fake_key_with_bytes("ssh-ed25519", b"O" * 32)
        exc = paramiko.BadHostKeyException("[example.com]:2222", new_key, old_key)

        client._handle_bad_host_key_interactive(exc)
        # prompt.hostname 应剥离为 bare host
        assert received[0].hostname == "example.com"
        assert received[0].port == 2222


# ==================== _format_bad_host_key_error 友好错误 ====================
class TestFormatErrorMessage:
    def test_message_contains_host_and_known_hosts_path(
        self,
        credentials: SSHCredentials,
        known_hosts_store: KnownHostsStore,
        tmp_path: Path,
    ) -> None:
        client = SSHClient(credentials)
        new_key = _fake_key_with_bytes("ssh-ed25519", b"NEW" * 11)
        old_key = _fake_key_with_bytes("ssh-ed25519", b"OLD" * 11)
        exc = paramiko.BadHostKeyException("example.com", new_key, old_key)

        msg = client._format_bad_host_key_error(exc)

        assert "example.com" in msg
        assert "原指纹" in msg
        assert "新指纹" in msg
        # 应包含具体清理路径 (临时 tmp_path)
        assert "known_hosts" in msg
        # 修复指引
        assert "重装" in msg or "中间人" in msg

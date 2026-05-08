# -*- coding: utf-8 -*-
"""[`InteractiveHostKeyPolicy`](src/core/remote/host_key_policy.py) 单元测试 (P4 W1·F5.1).

覆盖:

- 指纹计算 (SHA256 / MD5) 与 OpenSSH 工具输出格式一致
- ``HostKeyDecision.TRUST_SAVE`` 路径: client.host_keys + 磁盘 known_hosts 都被写入
- ``TRUST_ONCE``: 不写盘, 不修改 client.host_keys
- ``REJECT``: 抛 ``paramiko.SSHException``
- 回调抛异常: 也视为 REJECT
- ``KnownHostsStore.contains`` 在 add+save+重新 load 后仍然命中
- 非 22 端口走 ``[host]:port`` 条目格式
- ``import_from_openssh`` 不存在 ``~/.ssh/known_hosts`` 时返回 0
"""
from __future__ import annotations

# 标准库导入
from pathlib import Path

# 第三方库导入
import paramiko
import pytest

# 项目内模块导入
from src.core.remote.host_key_policy import (
    HostKeyDecision,
    HostKeyPrompt,
    InteractiveHostKeyPolicy,
    KnownHostsStore,
    compute_md5_fingerprint,
    compute_sha256_fingerprint,
)


# ==================== fixtures ====================
@pytest.fixture(scope="module")
def fake_key() -> paramiko.PKey:
    """生成一把临时 RSA 密钥, 模拟服务器 host key.

    paramiko 4.x 中 ``Ed25519Key.generate`` 不存在; 用 RSA 1024 (测试用够快).
    ``module`` 作用域避免每个用例都重新算一次密钥.
    """
    return paramiko.RSAKey.generate(1024)


@pytest.fixture
def known_hosts_store(tmp_path: Path) -> KnownHostsStore:
    return KnownHostsStore(path=tmp_path / "ssh" / "known_hosts")


# ==================== 指纹计算 ====================
def test_sha256_fingerprint_format(fake_key: paramiko.PKey) -> None:
    fp = compute_sha256_fingerprint(fake_key)
    assert fp.startswith("SHA256:")
    body = fp[len("SHA256:"):]
    # base64 (no padding) 长度 = ceil(32 / 3) * 4 - padding = 44 - 1 = 43
    assert len(body) == 43
    # 不应含 = 填充
    assert "=" not in body


def test_md5_fingerprint_format(fake_key: paramiko.PKey) -> None:
    fp = compute_md5_fingerprint(fake_key)
    parts = fp.split(":")
    assert len(parts) == 16
    for part in parts:
        assert len(part) == 2
        int(part, 16)  # 应能解析为十六进制


def test_fingerprints_are_stable_for_same_key(fake_key: paramiko.PKey) -> None:
    assert compute_sha256_fingerprint(fake_key) == compute_sha256_fingerprint(fake_key)
    assert compute_md5_fingerprint(fake_key) == compute_md5_fingerprint(fake_key)


# ==================== TRUST_SAVE ====================
def test_trust_save_writes_to_client_and_disk(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    decisions: list[HostKeyPrompt] = []

    def callback(prompt: HostKeyPrompt) -> HostKeyDecision:
        decisions.append(prompt)
        return HostKeyDecision.TRUST_SAVE

    policy = InteractiveHostKeyPolicy(
        callback=callback,
        store=known_hosts_store,
        port=22,
    )
    client = paramiko.SSHClient()

    policy.missing_host_key(client, "example.com", fake_key)

    # callback 收到完整 prompt
    assert len(decisions) == 1
    p = decisions[0]
    assert p.hostname == "example.com"
    assert p.port == 22
    assert p.key_type == fake_key.get_name()
    assert p.fingerprint_sha256.startswith("SHA256:")

    # client 内存集合已注入
    assert client.get_host_keys().lookup("example.com") is not None

    # 磁盘 known_hosts 已落盘
    assert known_hosts_store.path.exists()

    # 重新 load 后 contains 仍命中
    fresh = KnownHostsStore(path=known_hosts_store.path)
    assert fresh.contains("example.com")


def test_trust_save_with_non_default_port_uses_bracket_notation(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    """22 以外端口在 known_hosts 中以 ``[host]:port`` 格式存储."""
    policy = InteractiveHostKeyPolicy(
        callback=lambda _p: HostKeyDecision.TRUST_SAVE,
        store=known_hosts_store,
        port=2222,
    )
    client = paramiko.SSHClient()

    policy.missing_host_key(client, "example.com", fake_key)

    # 直接看 known_hosts 文件第一列
    text = known_hosts_store.path.read_text(encoding="ascii")
    assert "[example.com]:2222" in text
    # contains 也应正确命中
    fresh = KnownHostsStore(path=known_hosts_store.path)
    assert fresh.contains("example.com", port=2222)


def test_trust_save_handles_paramiko_pre_formatted_hostname(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    """回归: paramiko 在 ``port != 22`` 时把 hostname 预先包装成 ``[host]:port``,
    传进 ``missing_host_key`` 时**不能**再走一遍 ``_format_host_entry``,
    否则会写出 ``[[host]:port]:port`` 这种永远 lookup 不到的废条目, 表现为
    "每次连接都弹指纹确认对话框".
    """
    captured: list[HostKeyPrompt] = []
    policy = InteractiveHostKeyPolicy(
        callback=lambda p: (captured.append(p), HostKeyDecision.TRUST_SAVE)[1],
        store=known_hosts_store,
        port=2222,
    )
    client = paramiko.SSHClient()

    # 模拟 paramiko 4.x 的真实行为: 直接把 ``[host]:port`` 传进来
    policy.missing_host_key(client, "[example.com]:2222", fake_key)

    # prompt 中暴露的 hostname 应该是 bare host, 不带 [] / port 重复
    assert len(captured) == 1
    assert captured[0].hostname == "example.com"
    assert captured[0].port == 2222

    # 落盘 known_hosts 必须是单层 ``[example.com]:2222``, 不能是 ``[[example.com]:2222]:2222``
    text = known_hosts_store.path.read_text(encoding="ascii")
    assert "[example.com]:2222" in text
    assert "[[example.com]" not in text

    # 重新 load 之后, 用 bare host + port 应当命中, 下次连接不会再弹窗
    fresh = KnownHostsStore(path=known_hosts_store.path)
    assert fresh.contains("example.com", port=2222)


def test_trust_save_save_failure_does_not_propagate(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    """save() 抛 OSError 不应让 missing_host_key 抛 (本次连接已无问题).

    ``KnownHostsStore`` 用 ``slots=True`` dataclass, 不能直接 setattr 实例属性;
    走 class-level patch.
    """
    def boom(self, *args, **kwargs):
        raise OSError("disk full")

    monkeypatch.setattr(KnownHostsStore, "save", boom)
    policy = InteractiveHostKeyPolicy(
        callback=lambda _p: HostKeyDecision.TRUST_SAVE,
        store=known_hosts_store,
        port=22,
    )
    client = paramiko.SSHClient()

    # 不抛
    policy.missing_host_key(client, "example.com", fake_key)
    assert client.get_host_keys().lookup("example.com") is not None


# ==================== TRUST_ONCE ====================
def test_trust_once_does_not_write_disk_or_client_keys(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    policy = InteractiveHostKeyPolicy(
        callback=lambda _p: HostKeyDecision.TRUST_ONCE,
        store=known_hosts_store,
        port=22,
    )
    client = paramiko.SSHClient()

    policy.missing_host_key(client, "example.com", fake_key)

    # 不写盘
    assert not known_hosts_store.path.exists()
    # 不入 client.host_keys
    assert client.get_host_keys().lookup("example.com") is None


# ==================== REJECT ====================
def test_reject_raises_ssh_exception(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    policy = InteractiveHostKeyPolicy(
        callback=lambda _p: HostKeyDecision.REJECT,
        store=known_hosts_store,
        port=22,
    )
    client = paramiko.SSHClient()

    with pytest.raises(paramiko.SSHException) as exc_info:
        policy.missing_host_key(client, "example.com", fake_key)

    assert "拒绝" in str(exc_info.value)


def test_callback_exception_treated_as_reject(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    def boom(_p: HostKeyPrompt) -> HostKeyDecision:
        raise RuntimeError("UI thread crashed")

    policy = InteractiveHostKeyPolicy(
        callback=boom,
        store=known_hosts_store,
        port=22,
    )
    client = paramiko.SSHClient()

    with pytest.raises(paramiko.SSHException) as exc_info:
        policy.missing_host_key(client, "example.com", fake_key)

    assert "对话框异常" in str(exc_info.value)


# ==================== KnownHostsStore ====================
def test_known_hosts_store_round_trip(
    fake_key: paramiko.PKey, tmp_path: Path
) -> None:
    store = KnownHostsStore(path=tmp_path / "ssh" / "known_hosts")
    assert not store.contains("example.com")

    store.add("example.com", 22, fake_key)
    assert store.contains("example.com")
    store.save()

    fresh = KnownHostsStore(path=tmp_path / "ssh" / "known_hosts")
    assert fresh.contains("example.com")


def test_known_hosts_store_corrupt_file_falls_back_to_empty(tmp_path: Path) -> None:
    """known_hosts 文件损坏时不应让 load 抛, 应返回空集."""
    path = tmp_path / "ssh" / "known_hosts"
    path.parent.mkdir(parents=True)
    path.write_text("this is not a valid known_hosts line\n", encoding="ascii")

    store = KnownHostsStore(path=path)
    # contains 不抛
    assert store.contains("example.com") is False


def test_set_port_updates_subsequent_prompts(
    fake_key: paramiko.PKey, known_hosts_store: KnownHostsStore
) -> None:
    """``SSHClient`` 在 connect 前调用 ``set_port`` 应反映到下一次 callback."""
    captured: list[int] = []
    policy = InteractiveHostKeyPolicy(
        callback=lambda p: (captured.append(p.port), HostKeyDecision.TRUST_ONCE)[1],
        store=known_hosts_store,
        port=22,
    )
    policy.set_port(2222)

    client = paramiko.SSHClient()
    policy.missing_host_key(client, "example.com", fake_key)

    assert captured == [2222]


def test_import_from_openssh_returns_zero_when_legacy_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """``~/.ssh/known_hosts`` 不存在时静默返回 0, 不抛."""
    fake_home = tmp_path / "fake_home"
    fake_home.mkdir()
    monkeypatch.setattr(Path, "home", lambda: fake_home)

    store = KnownHostsStore(path=tmp_path / "ssh" / "known_hosts")
    assert store.import_from_openssh() == 0

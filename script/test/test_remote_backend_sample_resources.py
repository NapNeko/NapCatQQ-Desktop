# -*- coding: utf-8 -*-
"""[`RemoteBackend.sample_resources`](src/core/operation/remote_backend.py)
+ [`parse_sample_output`](src/core/remote/resource_monitor.py) 单测 (P4 W2·F3).

设计要点
========

仅覆盖纯解析层 + RemoteBackend 上的 ``sample_resources()`` 调度逻辑;
不真打 SSH, 通过 ``monkeypatch`` 注入伪 ``self._exec_backend.run`` 和
``self._ensure_connected``.
"""
from __future__ import annotations

# 标准库导入
from types import SimpleNamespace

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.operation.remote_backend import RemoteBackend
from src.core.remote.resource_monitor import (
    ResourceSample,
    parse_sample_output,
)


# ==================== parse_sample_output ====================
def test_parse_sample_output_valid_full() -> None:
    output = "CPU 12.5\nMEM 38.2\nDISK 47\nLOAD 0.42\n"
    sample = parse_sample_output(output)
    assert sample is not None
    assert sample.cpu_percent == pytest.approx(12.5)
    assert sample.mem_percent == pytest.approx(38.2)
    assert sample.disk_percent == pytest.approx(47.0)
    assert sample.load_avg_1 == pytest.approx(0.42)
    assert sample.raw == {"CPU": "12.5", "MEM": "38.2", "DISK": "47", "LOAD": "0.42"}
    assert sample.timestamp > 0


def test_parse_sample_output_missing_load_still_valid() -> None:
    """LOAD 缺失不应导致整次采样作废 — load_avg_1 仅用于辅助显示."""
    output = "CPU 5.0\nMEM 10.0\nDISK 20\n"
    sample = parse_sample_output(output)
    assert sample is not None
    assert sample.load_avg_1 is None


def test_parse_sample_output_missing_cpu_returns_none() -> None:
    output = "MEM 10.0\nDISK 20\nLOAD 0.1\n"
    assert parse_sample_output(output) is None


def test_parse_sample_output_missing_mem_returns_none() -> None:
    assert parse_sample_output("CPU 5\nDISK 20\nLOAD 0.1\n") is None


def test_parse_sample_output_missing_disk_returns_none() -> None:
    assert parse_sample_output("CPU 5\nMEM 10\nLOAD 0.1\n") is None


def test_parse_sample_output_invalid_float_returns_none() -> None:
    output = "CPU not_a_number\nMEM 10\nDISK 20\nLOAD 0.1\n"
    assert parse_sample_output(output) is None


def test_parse_sample_output_empty_returns_none() -> None:
    assert parse_sample_output("") is None
    assert parse_sample_output(None) is None  # type: ignore[arg-type]


def test_parse_sample_output_extra_garbage_lines_tolerated() -> None:
    """top -bn1 真实输出可能掺杂日志行, 解析需鲁棒."""
    output = (
        "warning: cannot read tty\n"
        "CPU 10.0\n"
        "some unrelated output\n"
        "MEM 20.0\n"
        "DISK 30\n"
        "LOAD 0.5\n"
        "trailing junk\n"
    )
    sample = parse_sample_output(output)
    assert sample is not None
    assert sample.cpu_percent == pytest.approx(10.0)
    assert sample.disk_percent == pytest.approx(30.0)


# ==================== RemoteBackend.sample_resources ====================
def _make_backend(monkeypatch: pytest.MonkeyPatch, *, run_result, raise_on_run: Exception | None = None):
    """构造一个最小化的 RemoteBackend, 旁路 SSHClient / 连接行为.

    我们只关心 ``sample_resources()`` 的调度逻辑, 不真实连接.
    """
    backend = RemoteBackend.__new__(RemoteBackend)  # 避开 __init__ 的 SSHClient 创建
    monkeypatch.setattr(backend, "_ensure_connected", lambda: None, raising=False)

    def _run(_command, *, timeout=None):
        del timeout
        if raise_on_run is not None:
            raise raise_on_run
        return run_result

    fake_exec = SimpleNamespace(run=_run)
    monkeypatch.setattr(backend, "_exec_backend", fake_exec, raising=False)
    return backend


def test_remote_backend_sample_resources_returns_sample_on_success(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fake_result = SimpleNamespace(ok=True, stdout="CPU 8\nMEM 22\nDISK 41\nLOAD 0.05\n", stderr="")
    backend = _make_backend(monkeypatch, run_result=fake_result)

    sample = backend.sample_resources()
    assert isinstance(sample, ResourceSample)
    assert sample.cpu_percent == pytest.approx(8.0)
    assert sample.mem_percent == pytest.approx(22.0)
    assert sample.disk_percent == pytest.approx(41.0)


def test_remote_backend_sample_resources_returns_none_on_nonzero_exit(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fake_result = SimpleNamespace(ok=False, stdout="", stderr="permission denied")
    backend = _make_backend(monkeypatch, run_result=fake_result)
    assert backend.sample_resources() is None


def test_remote_backend_sample_resources_returns_none_on_exception(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = _make_backend(
        monkeypatch,
        run_result=None,
        raise_on_run=ConnectionError("ssh transport closed"),
    )
    # sample_resources 必须吞掉异常返回 None, 让上层 worker 走退避
    assert backend.sample_resources() is None


def test_remote_backend_sample_resources_returns_none_on_unparseable_output(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fake_result = SimpleNamespace(ok=True, stdout="garbage that does not match\n", stderr="")
    backend = _make_backend(monkeypatch, run_result=fake_result)
    assert backend.sample_resources() is None

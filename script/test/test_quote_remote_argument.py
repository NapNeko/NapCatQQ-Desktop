# -*- coding: utf-8 -*-
"""[`SSHClient._quote_remote_argument`](src/core/remote/ssh_client.py) 单测
(P5 安全收尾 F2.2).

历史实现把 ``$HOME`` 前缀的路径**完整套双引号** (``"$HOME/..."``), 让 bash 展开
``$HOME`` 的同时, ``$()`` / 反引号 / ``$VAR`` 也会被一并展开 — 这就是
``workspace_dir`` 命令注入路径的最后一公里. 新实现把 ``$HOME`` 单独留双引号,
余下后缀强制走 ``shlex.quote``.

注: 本模块仅做 quote 算法验证, 不发起真实 SSH; 也不需要 paramiko 在线.
"""
from __future__ import annotations

# 标准库导入
import os
import shutil
import subprocess
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.remote.ssh_client import SSHClient


def _find_bash() -> str | None:
    """优先 Git Bash, 跳过 WSL stub."""
    explicit_candidates = [
        r"E:\Program Files\Git\usr\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
        r"C:\Program Files (x86)\Git\usr\bin\bash.exe",
        r"C:\msys64\usr\bin\bash.exe",
        r"C:\cygwin64\bin\bash.exe",
    ]
    for path in explicit_candidates:
        if Path(path).exists():
            return path
    for path in os.environ.get("PATH", "").split(os.pathsep):
        if not path:
            continue
        for name in ("bash.exe", "bash"):
            candidate = Path(path) / name
            if candidate.exists() and "WindowsApps" not in str(candidate):
                return str(candidate)
    if os.name != "nt":
        return shutil.which("bash")
    return None


_BASH = _find_bash()


def _bash_eval(expr: str) -> str:
    """在 bash 中 ``echo`` 一段表达式, 返回展开后字符串.

    用 ``HOME=/tmp/fake-home`` 让 ``$HOME`` 展开结果可预测.
    """
    if _BASH is None:
        pytest.skip("bash 不可用")
    proc = subprocess.run(
        [_BASH, "-c", f'printf "%s" {expr}'],
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=10,
        env={**os.environ, "LC_ALL": "C", "HOME": "/tmp/fake-home"},
    )
    assert proc.returncode == 0, f"bash 退出 {proc.returncode}: stderr={proc.stderr}"
    return proc.stdout


# ==================== 静态行为 ====================
def test_quote_for_absolute_path_uses_shlex() -> None:
    """绝对路径走 shlex.quote, 不会被双引号包."""
    quoted = SSHClient._quote_remote_argument("/opt/napcat")  # noqa: SLF001
    # shlex.quote 对纯字母数字 + / 路径直接返回原值 (不加引号)
    assert quoted == "/opt/napcat" or quoted == "'/opt/napcat'"


def test_quote_for_path_with_metachar_uses_shlex() -> None:
    """含 shell 元字符的绝对路径必须被 shlex 严格转义."""
    quoted = SSHClient._quote_remote_argument("/opt/has space")  # noqa: SLF001
    # shlex.quote 应该把它包起来
    assert quoted.startswith("'")
    assert quoted.endswith("'")


def test_quote_for_home_only() -> None:
    """``$HOME`` (无后缀) 应只走双引号让 bash 展开."""
    quoted = SSHClient._quote_remote_argument("$HOME")  # noqa: SLF001
    assert quoted == '"$HOME"'


def test_quote_for_home_with_subpath_separates_prefix_and_suffix() -> None:
    """``$HOME/Napcat`` -> ``"$HOME"`` + shlex 转义后缀.

    后缀 ``/Napcat`` 不含特殊字符, shlex 会原样返回, 拼接后是 ``"$HOME"/Napcat``.
    """
    quoted = SSHClient._quote_remote_argument("$HOME/Napcat")  # noqa: SLF001
    assert quoted.startswith('"$HOME"')
    # 后缀部分必须出现 (可能加了引号)
    suffix_segment = quoted[len('"$HOME"'):]
    assert "/Napcat" in suffix_segment


# ==================== bash 真实展开行为 ====================
def test_home_path_expands_in_bash() -> None:
    """quote 出来的 ``"$HOME"/Napcat`` 经 bash echo 后应展开为 ``/tmp/fake-home/Napcat``."""
    quoted = SSHClient._quote_remote_argument("$HOME/Napcat")  # noqa: SLF001
    out = _bash_eval(quoted)
    assert out == "/tmp/fake-home/Napcat"


def test_home_path_with_command_substitution_attempt_is_literal() -> None:
    """``$HOME/Napcat$(rm)`` quote 后, bash 仅展开 $HOME, ``$(rm)`` 保持字面.

    这是 P5 F2.2 修复的核心场景: 后缀里的 ``$()`` 被 shlex 包成单引号,
    bash 不再二次展开.
    """
    pwned_marker = "/tmp/_napcat_test_q_pwned_marker"
    Path(pwned_marker).unlink(missing_ok=True)

    payload = f"$HOME/Napcat$(touch {pwned_marker})"
    quoted = SSHClient._quote_remote_argument(payload)  # noqa: SLF001
    out = _bash_eval(quoted)

    # 字面 "$(touch ...)" 必须仍在输出里
    assert "$(touch " in out, f"命令替换被执行, out={out!r}"
    # 真实 marker 不应被创建
    assert not Path(pwned_marker).exists(), "命令替换被执行, _quote_remote_argument 仍有漏洞"


def test_home_path_with_backtick_attempt_is_literal() -> None:
    """``$HOME/Napcat`whoami`` 反引号后缀也应被字面化."""
    payload = "$HOME/Napcat`whoami`"
    quoted = SSHClient._quote_remote_argument(payload)  # noqa: SLF001
    out = _bash_eval(quoted)
    assert "`whoami`" in out


def test_home_path_with_dollar_var_in_suffix_is_literal() -> None:
    """``$HOME/$USER`` 中只有最前面的 ``$HOME`` 展开, 后缀里的 ``$USER`` 保留."""
    payload = "$HOME/$USER/foo"
    quoted = SSHClient._quote_remote_argument(payload)  # noqa: SLF001
    out = _bash_eval(quoted)
    assert out == "/tmp/fake-home/$USER/foo"


def test_absolute_path_with_command_substitution_attempt_is_literal() -> None:
    """非 ``$HOME`` 起头的恶意路径同样被 shlex 整体单引号包."""
    pwned_marker = "/tmp/_napcat_test_q_abs_pwned"
    Path(pwned_marker).unlink(missing_ok=True)

    payload = f"/opt/Napcat$(touch {pwned_marker})"
    quoted = SSHClient._quote_remote_argument(payload)  # noqa: SLF001
    out = _bash_eval(quoted)
    assert "$(touch " in out
    assert not Path(pwned_marker).exists()

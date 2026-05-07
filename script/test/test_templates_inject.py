# -*- coding: utf-8 -*-
"""[`inject_script_variables`](src/core/remote/templates.py) 安全注入语法
单测 (P5 安全收尾 F2.1).

关键不变量:
- ``$HOME/...`` 形式必须仍然让远端 bash 展开 ``$HOME`` 为实际 home 目录
- 含 ``$()`` / 反引号 / ``$VAR`` 的恶意值**绝不**触发命令替换或变量展开
- 含单引号的合法值(罕见但应当兼容)经过往返保留
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
from src.core.remote.templates import inject_script_variables


def _find_bash() -> str | None:
    """优先 Git Bash, 跳过 WSL stub (WindowsApps 路径)."""
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
pytestmark = pytest.mark.skipif(_BASH is None, reason="bash 不可用, 跳过注入语法测试")


def _eval_in_bash(injected_lines: str, var_name: str) -> str:
    """把 injected_lines 注入空脚本并 echo ``$<var_name>``, 返回 bash 求值后的字符串."""
    script = (
        "#!/usr/bin/env bash\n"
        "set -uo pipefail\n"
        f"{injected_lines}\n"
        f'printf "%s" "${{{var_name}}}"\n'
    )
    proc = subprocess.run(
        [_BASH, "-c", script],
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=10,
        env={**os.environ, "LC_ALL": "C", "HOME": "/tmp/fake-home"},
    )
    assert proc.returncode == 0, f"bash 退出 {proc.returncode}: stderr={proc.stderr}"
    return proc.stdout


def _inject_one(key: str, value: str) -> str:
    """方便: 注入单个变量, 返回 bash 拼装后的字面文本."""
    return inject_script_variables("#!/usr/bin/env bash\n", {key: value})


# ==================== 注入语法基础 ====================
def test_inject_preserves_shebang_at_top() -> None:
    """注入变量后 shebang 必须仍在第一行."""
    text = inject_script_variables("#!/usr/bin/env bash\necho hi\n", {"foo": "bar"})
    assert text.splitlines()[0] == "#!/usr/bin/env bash"


def test_inject_emits_lines_after_shebang() -> None:
    """注入位置必须紧跟 shebang, 不丢失 ``set -euo pipefail`` 等后续行."""
    text = inject_script_variables("#!/usr/bin/env bash\nset -euo pipefail\n", {"foo": "bar"})
    lines = text.splitlines()
    assert lines[0] == "#!/usr/bin/env bash"
    assert "foo=" in lines[1]
    assert lines[2] == "set -euo pipefail"


# ==================== $HOME 展开行为 ====================
def test_home_prefix_expands_in_bash() -> None:
    """``$HOME/Napcat`` 注入后, bash 求值必须仍展开 $HOME."""
    out = _eval_in_bash(_inject_one("workspace_dir", "$HOME/Napcat"), "workspace_dir")
    assert out == "/tmp/fake-home/Napcat"


def test_home_only_expands_in_bash() -> None:
    """单独 ``$HOME`` (无后缀) 也必须展开."""
    out = _eval_in_bash(_inject_one("home_dir", "$HOME"), "home_dir")
    assert out == "/tmp/fake-home"


def test_home_with_subpath_expands_in_bash() -> None:
    """``$HOME/foo/bar`` 这种多级路径也要正确展开."""
    out = _eval_in_bash(_inject_one("p", "$HOME/foo/bar"), "p")
    assert out == "/tmp/fake-home/foo/bar"


def test_absolute_path_without_home_is_literal() -> None:
    """``/opt/napcat`` 这种绝对路径应原样保留, 不做任何展开."""
    out = _eval_in_bash(_inject_one("p", "/opt/napcat"), "p")
    assert out == "/opt/napcat"


# ==================== 命令注入防御 ====================
def test_command_substitution_in_value_is_blocked() -> None:
    """``$HOME/Napcat$(touch /tmp/PWNED)`` 注入后 bash 取值仍是字面字符串, 不触发命令替换."""
    pwned_marker = "/tmp/_napcat_test_pwned_marker"
    # 攻击 payload 故意尝试执行 touch
    payload = f"$HOME/Napcat$(touch {pwned_marker})"

    # 先确保标记不存在
    Path(pwned_marker).unlink(missing_ok=True)
    out = _eval_in_bash(_inject_one("workspace_dir", payload), "workspace_dir")

    # 关键断言 1: 值里仍包含字面 "$(touch ...)" 字符串 (未被 bash 当成命令替换执行)
    assert "$(touch " in out, f"命令替换未被阻止, out={out!r}"
    # 关键断言 2: 标记文件不存在 (touch 没被执行)
    assert not Path(pwned_marker).exists(), "命令替换被执行, 注入语法有漏洞!"


def test_backtick_substitution_in_value_is_blocked() -> None:
    """反引号命令替换同样应被阻止."""
    payload = "$HOME/Napcat`whoami`"
    out = _eval_in_bash(_inject_one("p", payload), "p")
    assert "`whoami`" in out


def test_dollar_var_in_suffix_is_literal() -> None:
    """``$HOME/$USER/foo`` 中除前缀 ``$HOME`` 外的 ``$USER`` 应保留为字面 (非展开)."""
    payload = "$HOME/$USER/foo"
    out = _eval_in_bash(_inject_one("p", payload), "p")
    # $HOME 被展开, $USER 保留字面
    assert out == "/tmp/fake-home/$USER/foo"


def test_semicolon_chain_in_value_is_literal() -> None:
    """注入值含 ``;`` 不会触发分隔符执行."""
    payload = "$HOME/Napcat;rm -rf /"
    out = _eval_in_bash(_inject_one("p", payload), "p")
    assert out == "/tmp/fake-home/Napcat;rm -rf /"


# ==================== 单引号处理 ====================
def test_value_with_single_quote_roundtrips() -> None:
    """合法值含单引号也应可注入并取回原值."""
    payload = "/opt/it's mine"
    out = _eval_in_bash(_inject_one("p", payload), "p")
    assert out == payload


# ==================== 双引号处理 ====================
def test_value_with_double_quote_roundtrips() -> None:
    """值含双引号 (虽然 LinuxCorePaths 校验会禁掉, 模板自身仍要稳)."""
    payload = '/opt/path"with"quote'
    out = _eval_in_bash(_inject_one("p", payload), "p")
    assert out == payload

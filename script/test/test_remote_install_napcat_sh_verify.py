# -*- coding: utf-8 -*-
"""脚本级单测: ``verify_napcat_archive_sha512`` (P5 安全收尾 F1.4).

直接用 bash subprocess 跑 ``remote_install_napcat.sh`` 中 SHA512 校验函数, 验证:

- 期望 hash 一致 -> exit 0
- 期望 hash 不一致 -> exit 36, archive 被删
- 缺失 ``$NAPCAT_EXPECTED_SHA512`` -> exit 0 (warn skip, 兼容老客户端)
- ``sha512sum`` 缺失但 ``openssl`` 可用 -> exit 0
- 两个工具都缺失且要求校验 -> exit 36, archive 被删

跨平台: Windows 必须装 Git Bash / WSL bash. 找不到 bash -> 跳过整个文件.
"""
from __future__ import annotations

# 标准库导入
import hashlib
import os
import shutil
import subprocess
from pathlib import Path

# 第三方库导入
import pytest


def _find_bash() -> str | None:
    """优先挑选**真实的 POSIX bash**, 避开 WSL stub.

    ``shutil.which("bash")`` 在 Windows 上常常先命中
    ``C:\\Users\\<u>\\AppData\\Local\\Microsoft\\WindowsApps\\bash.exe``
    — 这是 WSL stub, 它会把 Windows 路径当作 Linux 路径解析, 导致测试中
    ``sha512sum '<windows-path>'`` 拿不到文件. 优先选 Git Bash / MSYS2 /
    cygwin 这种真正在 Windows 文件系统上工作的 bash.
    """
    # Git Bash / MSYS2 / Cygwin 常见安装路径
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

    # 退化: 扫 PATH 但跳过 WindowsApps 下的 WSL stub
    for path in os.environ.get("PATH", "").split(os.pathsep):
        if not path:
            continue
        # 检测 .exe 与无扩展名两种形态
        for name in ("bash.exe", "bash"):
            candidate = Path(path) / name
            if candidate.exists() and "WindowsApps" not in str(candidate):
                return str(candidate)

    # 最后退路: 如果当前不在 Windows, 直接信 shutil.which
    if os.name != "nt":
        candidate = shutil.which("bash")
        if candidate:
            return candidate
    return None


_BASH = _find_bash()
pytestmark = pytest.mark.skipif(_BASH is None, reason="bash 不可用, 跳过脚本级测试")


PROJECT_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = PROJECT_ROOT / "src" / "resource" / "script" / "remote_install_napcat.sh"


def _extract_verify_function() -> str:
    """从 install 脚本中切出 ``verify_napcat_archive_sha512`` 函数体.

    按 ``verify_napcat_archive_sha512()`` 起 / 第一个独立 ``}`` 终止 (函数末尾)
    切片. 该函数自身无嵌套 ``}``, 切片足够稳健.
    """
    text = SCRIPT_PATH.read_text(encoding="utf-8")
    start = text.find("verify_napcat_archive_sha512()")
    assert start != -1, "未在脚本中找到 verify_napcat_archive_sha512 函数定义"
    end = text.find("\n}\n", start)
    assert end != -1, "未在脚本中找到 verify_napcat_archive_sha512 函数闭合"
    return text[start : end + 3]


def _run_verify(
    *,
    archive_content: bytes,
    expected_env: str | None,
    extra_setup: str = "",
    sha512sum_available: bool = True,
    openssl_available: bool = True,
    tmp_path: Path,
) -> tuple[int, str, bool]:
    """运行 ``verify_napcat_archive_sha512`` 并返回 ``(exit_code, output, archive_still_exists)``."""
    archive = tmp_path / "NapCat.Shell.zip"
    archive.write_bytes(archive_content)

    func_body = _extract_verify_function()

    # 准备 PATH 屏蔽: 若指定 sha512sum/openssl 不可用, 把 bash 内的 ``command -v``
    # 改成永远返回非零. 用 ``command()`` shell builtin override 实现.
    overrides: list[str] = []
    if not sha512sum_available:
        overrides.append('sha512sum() { return 127; }')
        overrides.append("command() {\n  if [ \"$1\" = '-v' ] && [ \"$2\" = 'sha512sum' ]; then\n    return 1\n  fi\n  builtin command \"$@\"\n}")
    if not openssl_available:
        overrides.append('openssl() { return 127; }')
        # 当两个都不可用时, ``command -v`` 都要返回失败; 用复合判断
        overrides.append(
            "command() {\n"
            "  if [ \"$1\" = '-v' ]; then\n"
            "    if [ \"$2\" = 'sha512sum' ] || [ \"$2\" = 'openssl' ]; then\n"
            "      return 1\n"
            "    fi\n"
            "  fi\n"
            "  builtin command \"$@\"\n"
            "}"
        )

    env_set = ""
    if expected_env is not None:
        env_set = f'export NAPCAT_EXPECTED_SHA512={shell_quote(expected_env)}\n'

    # 拼装 bash 脚本: 预设 log_* / log_progress 桩 + override + 函数 + 调用
    script = (
        "#!/usr/bin/env bash\n"
        "set -uo pipefail\n"  # 故意不设 -e: 让 exit 36 不会被 trap 干扰
        'log_info()  { printf "[INFO] %s\\n"  "$*"; }\n'
        'log_warn()  { printf "[WARN] %s\\n"  "$*"; }\n'
        'log_error() { printf "[ERROR] %s\\n" "$*"; }\n'
        'log_progress() { :; }\n'
        + extra_setup
        + "\n"
        + "\n".join(overrides)
        + "\n"
        + env_set
        + func_body
        + "\n"
        + f'verify_napcat_archive_sha512 {shell_quote(str(archive))}\n'
    )

    proc = subprocess.run(
        [_BASH, "-c", script],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=30,
        env={**os.environ, "LC_ALL": "C"},
    )
    output = proc.stdout + proc.stderr
    return proc.returncode, output, archive.exists()


def shell_quote(value: str) -> str:
    """POSIX 单引号转义."""
    return "'" + value.replace("'", "'\\''") + "'"


def test_verify_passes_when_hash_matches(tmp_path: Path) -> None:
    content = b"hello napcat shell content"
    expected = hashlib.sha512(content).hexdigest()

    code, output, archive_kept = _run_verify(
        archive_content=content,
        expected_env=expected,
        tmp_path=tmp_path,
    )

    assert code == 0, f"output={output}"
    assert archive_kept, "校验通过时 archive 不应被删除"
    assert "sha512 verified ok" in output


def test_verify_passes_with_uppercase_expected(tmp_path: Path) -> None:
    """大写 hex 输入应被 ``tr`` 归一后比较成功."""
    content = b"hello napcat shell content"
    expected = hashlib.sha512(content).hexdigest().upper()

    code, output, archive_kept = _run_verify(
        archive_content=content,
        expected_env=expected,
        tmp_path=tmp_path,
    )

    assert code == 0, f"output={output}"
    assert archive_kept


def test_verify_fails_with_wrong_hash(tmp_path: Path) -> None:
    content = b"hello napcat shell content"
    expected = "ff" * 64  # 故意不匹配

    code, output, archive_kept = _run_verify(
        archive_content=content,
        expected_env=expected,
        tmp_path=tmp_path,
    )

    assert code == 36, f"output={output}"
    assert not archive_kept, "校验失败时 archive 必须被删除"
    assert "sha512 mismatch" in output


def test_verify_skips_when_env_missing(tmp_path: Path) -> None:
    """``NAPCAT_EXPECTED_SHA512`` 未设置 -> warn skip, exit 0, archive 保留."""
    code, output, archive_kept = _run_verify(
        archive_content=b"any content",
        expected_env=None,
        tmp_path=tmp_path,
    )

    assert code == 0
    assert archive_kept
    assert "integrity check skipped" in output


def test_verify_uses_openssl_when_sha512sum_missing(tmp_path: Path) -> None:
    """没有 ``sha512sum`` 时退化到 ``openssl dgst -sha512``."""
    if shutil.which("openssl") is None:
        pytest.skip("openssl 不可用, 跳过该子用例")

    content = b"hello napcat openssl path"
    expected = hashlib.sha512(content).hexdigest()

    code, output, archive_kept = _run_verify(
        archive_content=content,
        expected_env=expected,
        sha512sum_available=False,
        openssl_available=True,
        tmp_path=tmp_path,
    )

    assert code == 0, f"output={output}"
    assert archive_kept
    assert "sha512 verified ok" in output


def test_verify_fails_when_no_hash_tool_available(tmp_path: Path) -> None:
    """sha512sum 与 openssl 都不可用 + 要求校验 -> exit 36, archive 被删."""
    code, output, archive_kept = _run_verify(
        archive_content=b"any content",
        expected_env="aa" * 64,
        sha512sum_available=False,
        openssl_available=False,
        tmp_path=tmp_path,
    )

    assert code == 36, f"output={output}"
    assert not archive_kept
    assert "neither sha512sum nor openssl" in output

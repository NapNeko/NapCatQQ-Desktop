# -*- coding: utf-8 -*-
"""[`sanitize_text_for_export`](src/core/logging/crash_bundle.py) QQID 路径形式
脱敏补全 单测 (P5 安全收尾 F3.1).

历史脱敏正则只命中 ``QQID=12345`` / ``qq_id: 12345`` 形式. 本文件覆盖远程链路
打到 app.log 的实际形态:

- ``napcat_<qqid>.(log|json|pid|log.prev)``
- ``qq --no-sandbox -q <qqid>``
- ``ManagerNapCatQQProcess[<qqid>]``

替换语义: 仅替换数字段, 保留前后字面量, 走 ``mask_qqid`` 输出 ``***1217`` 风格.
"""
from __future__ import annotations

# 项目内模块导入
from src.core.logging.crash_bundle import sanitize_text_for_export


# ==================== 文件名形式 ====================
def test_napcat_log_filename_qqid_is_masked() -> None:
    text = "启动 napcat_3217681217.log 失败"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "napcat_***1217.log" in out


def test_napcat_json_filename_qqid_is_masked() -> None:
    text = "写入 napcat_3217681217.json 完成"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "napcat_***1217.json" in out


def test_napcat_pid_filename_qqid_is_masked() -> None:
    text = "PID 文件 napcat_3217681217.pid 已生成"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "napcat_***1217.pid" in out


def test_napcat_log_prev_filename_qqid_is_masked() -> None:
    """``.log.prev`` 后缀也要被脱敏."""
    text = "归档为 napcat_3217681217.log.prev"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "napcat_***1217.log.prev" in out


def test_onebot_filename_with_qqid_is_masked() -> None:
    """``onebot11_<qqid>.json`` 也按相同规则处理."""
    text = "渲染 onebot11_3217681217.json 完成"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "onebot11_***1217.json" in out


# ==================== 命令行形式 ====================
def test_cmdline_q_arg_is_masked() -> None:
    text = "qq --no-sandbox -q 3217681217"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "-q ***1217" in out


def test_cmdline_q_with_pgrep_is_masked() -> None:
    text = "pkill -f 'qq --no-sandbox -q 3217681217$'"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    # 应保留命令前后字面量
    assert "qq --no-sandbox -q ***1217" in out


# ==================== 中括号形式 ====================
def test_manager_bracket_qqid_is_masked() -> None:
    text = "ManagerNapCatQQProcess[3217681217] 退出"
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "ManagerNapCatQQProcess[***1217]" in out


# ==================== 不应误伤的场景 ====================
def test_short_numbers_below_5_digits_are_not_masked() -> None:
    """端口号 / 错误码等 4 位以下数字不应被处理."""
    text = "listen on :8080 with code 404"
    out = sanitize_text_for_export(text)
    assert "8080" in out
    assert "404" in out


def test_normal_filenames_without_qqid_pattern_are_not_touched() -> None:
    """不带 qqid 前缀的普通文件名不应被处理."""
    text = "loaded config.json and app.log"
    out = sanitize_text_for_export(text)
    assert "config.json" in out
    assert "app.log" in out


def test_napcat_without_underscore_qqid_is_not_touched() -> None:
    """``napcat.log`` (无 ``_<qqid>`` 段) 不触发文件名规则."""
    text = "tail -n 100 napcat.log"
    out = sanitize_text_for_export(text)
    assert "napcat.log" in out


# ==================== 综合场景 ====================
def test_combined_log_line_redacts_all_qqid_forms() -> None:
    """单行日志同时含多种 QQID 形式时, 所有数字段都应被脱敏."""
    text = (
        "ManagerNapCatQQProcess[3217681217] 启动 napcat_3217681217.log; "
        "命令: qq --no-sandbox -q 3217681217"
    )
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "***1217" in out
    assert out.count("***1217") == 3


def test_existing_kv_pattern_still_works() -> None:
    """历史 ``QQID=12345`` 形式的命中不应被 W3 改动破坏."""
    text = 'QQID="3217681217"'
    out = sanitize_text_for_export(text)
    assert "3217681217" not in out
    assert "***1217" in out


def test_existing_secret_pattern_still_works() -> None:
    """历史 ``token=`` 形式的命中不应被 W3 改动破坏."""
    text = 'Authorization: Bearer abc123secret'
    out = sanitize_text_for_export(text)
    assert "abc123secret" not in out
    assert "<redacted-secret>" in out

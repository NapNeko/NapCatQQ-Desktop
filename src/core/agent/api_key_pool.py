# -*- coding: utf-8 -*-
"""API 密钥池辅助函数.

`Provider.api_key_ref` 字段允许使用逗号分隔的多个密钥, 在每次请求时随机
挑选一个. 这避免了热点密钥的速率限制问题, 也不需要在适配器中关心多密钥逻辑. 

辅助函数:
- ``parse_api_keys``: 把逗号分隔字符串拆成密钥列表, 自动 strip 并跳过空项.
- ``pick_api_key``: 从密钥列表中随机选择一项 (单密钥时直接返回原值).
"""

from __future__ import annotations

import random


def parse_api_keys(api_key_ref: str) -> list[str]:
    """把逗号分隔的 api_key_ref 拆成密钥列表.

    Args:
        api_key_ref: 原始字符串, 形如 ``"sk-aaa,sk-bbb"`` 或 ``"sk-only"``.

    Returns:
        非空的密钥列表 (已 strip), 顺序保持原始输入. 若全部为空则返回 ``[""]``,
        以保证调用方能安全地继续走原有 "单密钥" 流程而不抛异常.
    """
    if not api_key_ref:
        return [""]
    keys = [k.strip() for k in api_key_ref.split(",") if k.strip()]
    return keys if keys else [api_key_ref]


def pick_api_key(api_key_ref: str) -> str:
    """从 api_key_ref 中挑选一个密钥用于本次请求.

    单密钥情况直接返回原字符串 (兼容老配置). 多密钥时随机选择一项,
    简单的随机轮转有助于分散流量, 避免命中单一密钥的速率限制.

    Args:
        api_key_ref: 原始字符串, 可能为单密钥或逗号分隔的多密钥.

    Returns:
        本次请求实际使用的单个密钥字符串.
    """
    keys = parse_api_keys(api_key_ref)
    if len(keys) == 1:
        return keys[0]
    return random.choice(keys)

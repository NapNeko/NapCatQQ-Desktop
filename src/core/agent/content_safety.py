# -*- coding: utf-8 -*-
"""内容安全 prompt 与内置知识加载模块.

加载只读的内容安全系统提示词和 NapCat 插件开发知识库。
这些 prompt 不可被用户修改。
"""

from __future__ import annotations

import os
from functools import lru_cache

# 内容安全 prompt 文件路径（相对于本模块）
_CONTENT_SAFETY_PROMPT_PATH = os.path.join(
    os.path.dirname(__file__), "prompts", "content_safety.md"
)

# NapCat 插件开发知识库 prompt 文件路径（相对于本模块）
_NAPCAT_PLUGIN_DEV_PROMPT_PATH = os.path.join(
    os.path.dirname(__file__), "prompts", "napcat_plugin_dev.md"
)


@lru_cache(maxsize=1)
def get_content_safety_prompt() -> str:
    """获取不可修改的内容安全系统提示词.

    该函数加载 prompts/content_safety.md 文件内容并缓存。
    返回的字符串为只读资源，不可被用户通过任何配置接口修改。

    Returns:
        内容安全系统提示词字符串.

    Raises:
        FileNotFoundError: 如果 content_safety.md 文件不存在（应用打包错误）.
    """
    with open(_CONTENT_SAFETY_PROMPT_PATH, "r", encoding="utf-8") as f:
        return f.read()


@lru_cache(maxsize=1)
def get_napcat_plugin_dev_prompt() -> str:
    """获取内置的 NapCat 插件开发知识库提示词.

    该函数加载 prompts/napcat_plugin_dev.md 文件内容并缓存。
    返回的字符串为只读资源，不可被用户通过任何配置接口修改。

    Returns:
        NapCat 插件开发知识库提示词字符串.

    Raises:
        FileNotFoundError: 如果 napcat_plugin_dev.md 文件不存在（应用打包错误）.
    """
    with open(_NAPCAT_PLUGIN_DEV_PROMPT_PATH, "r", encoding="utf-8") as f:
        return f.read()

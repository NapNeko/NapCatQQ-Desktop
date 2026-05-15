# -*- coding: utf-8 -*-
"""Protocol adapters package.

提供各 LLM 提供商的协议适配器实现, 将内部消息/工具表示
转换为提供商特定的 HTTP 请求载荷, 并解析流式响应为 StreamEvent 序列. 
"""

from src.core.agent.adapters.anthropic_adapter import AnthropicAdapter
from src.core.agent.adapters.azure_adapter import AzureAdapter
from src.core.agent.adapters.gemini_adapter import GeminiAdapter
from src.core.agent.adapters.openai_adapter import OpenAIAdapter

__all__ = ["AnthropicAdapter", "AzureAdapter", "GeminiAdapter", "OpenAIAdapter"]

# -*- coding: utf-8 -*-
"""默认供应商配置.

定义首次启动时预置的常用 AI 供应商列表. 
用户无需手动添加即可看到这些供应商并填入自己的 API Key 使用. 
所有默认供应商的 api_key_ref 为占位符 "sk-placeholder", enabled 默认为 False, 
需要用户填入真实 Key 后手动启用. 
"""

from __future__ import annotations

from src.core.agent.provider import ModelEntry, Provider

# 占位符 API Key, 用户需替换为真实值
_PLACEHOLDER_KEY = "sk-placeholder"


def get_default_providers() -> list[Provider]:
    """获取默认供应商列表.

    返回常用的 AI 供应商预置配置, 包含基本信息和代表性模型. 
    所有供应商默认禁用 (enabled=False) , 用户需填入 API Key 后启用. 

    Returns:
        预置供应商列表.
    """
    return [
        # --- 国际主流 ---
        Provider(
            provider_id="openai",
            name="OpenAI",
            api_base_url="https://api.openai.com/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="gpt-4o", display_name="GPT-4o", max_tokens=128000),
                ModelEntry(model_id="gpt-4o-mini", display_name="GPT-4o Mini", max_tokens=128000),
                ModelEntry(model_id="o3-mini", display_name="o3-mini", max_tokens=200000),
            ],
        ),
        Provider(
            provider_id="anthropic",
            name="Anthropic",
            api_base_url="https://api.anthropic.com/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="anthropic",
            models=[
                ModelEntry(model_id="claude-sonnet-4-20250514", display_name="Claude Sonnet 4", max_tokens=200000),
                ModelEntry(model_id="claude-3-5-haiku-20241022", display_name="Claude 3.5 Haiku", max_tokens=200000),
            ],
        ),
        Provider(
            provider_id="gemini",
            name="Gemini",
            api_base_url="https://generativelanguage.googleapis.com/v1beta",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="gemini",
            models=[
                ModelEntry(model_id="gemini-2.5-flash", display_name="Gemini 2.5 Flash", max_tokens=1048576),
                ModelEntry(model_id="gemini-2.5-pro", display_name="Gemini 2.5 Pro", max_tokens=1048576),
            ],
        ),
        Provider(
            provider_id="grok",
            name="Grok",
            api_base_url="https://api.x.ai/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="grok-3", display_name="Grok 3", max_tokens=131072),
                ModelEntry(model_id="grok-3-mini", display_name="Grok 3 Mini", max_tokens=131072),
            ],
        ),
        # --- 国内主流 ---
        Provider(
            provider_id="deepseek",
            name="DeepSeek",
            api_base_url="https://api.deepseek.com/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="deepseek-chat", display_name="DeepSeek V3", max_tokens=65536),
                ModelEntry(model_id="deepseek-reasoner", display_name="DeepSeek R1", max_tokens=65536),
            ],
        ),
        Provider(
            provider_id="zhipu",
            name="智谱 AI",
            api_base_url="https://open.bigmodel.cn/api/paas/v4",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="glm-4-plus", display_name="GLM-4 Plus", max_tokens=128000),
                ModelEntry(model_id="glm-4-flash", display_name="GLM-4 Flash", max_tokens=128000),
            ],
        ),
        Provider(
            provider_id="dashscope",
            name="通义千问 (百炼)",
            api_base_url="https://dashscope.aliyuncs.com/compatible-mode/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="qwen-max", display_name="Qwen Max", max_tokens=32768),
                ModelEntry(model_id="qwen-plus", display_name="Qwen Plus", max_tokens=131072),
                ModelEntry(model_id="qwen-turbo", display_name="Qwen Turbo", max_tokens=131072),
            ],
        ),
        Provider(
            provider_id="doubao",
            name="豆包 (火山引擎)",
            api_base_url="https://ark.cn-beijing.volces.com/api/v3",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="doubao-1.5-pro-32k", display_name="Doubao 1.5 Pro", max_tokens=32768),
            ],
        ),
        Provider(
            provider_id="moonshot",
            name="Moonshot AI (Kimi)",
            api_base_url="https://api.moonshot.cn/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="moonshot-v1-auto", display_name="Moonshot V1 Auto", max_tokens=128000),
            ],
        ),
        # --- 聚合平台 ---
        Provider(
            provider_id="silicon",
            name="SiliconFlow",
            api_base_url="https://api.siliconflow.cn/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="deepseek-ai/DeepSeek-V3", display_name="DeepSeek V3", max_tokens=65536),
                ModelEntry(model_id="deepseek-ai/DeepSeek-R1", display_name="DeepSeek R1", max_tokens=65536),
                ModelEntry(model_id="Qwen/Qwen3-235B-A22B", display_name="Qwen3 235B", max_tokens=131072),
            ],
        ),
        Provider(
            provider_id="openrouter",
            name="OpenRouter",
            api_base_url="https://openrouter.ai/api/v1",
            api_key_ref=_PLACEHOLDER_KEY,
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="openai/gpt-4o", display_name="GPT-4o", max_tokens=128000),
                ModelEntry(model_id="anthropic/claude-sonnet-4", display_name="Claude Sonnet 4", max_tokens=200000),
                ModelEntry(model_id="google/gemini-2.5-flash", display_name="Gemini 2.5 Flash", max_tokens=1048576),
            ],
        ),
        # --- 本地部署 ---
        Provider(
            provider_id="ollama",
            name="Ollama",
            api_base_url="http://localhost:11434/v1",
            api_key_ref="ollama",
            enabled=False,
            protocol_type="openai",
            models=[
                ModelEntry(model_id="llama3", display_name="Llama 3", max_tokens=8192),
            ],
        ),
    ]

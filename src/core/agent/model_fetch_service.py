# -*- coding: utf-8 -*-
"""模型获取服务.

根据供应商协议类型向对应 API 端点发送请求，解析模型列表响应，
并通过启发式规则推断模型能力，返回 ModelEntry 列表。

支持的协议类型:
- OpenAI: GET {api_base_url}/models, Authorization: Bearer {api_key}
- Anthropic: GET {api_base_url}/models, x-api-key + anthropic-version
- Gemini: GET {api_base_url}/models, x-goog-api-key
- Azure: GET {resource_endpoint}/openai/models?api-version={api_version}, api-key

分页支持:
- Gemini: nextPageToken → pageToken 参数，最多 10 页
- Anthropic: has_more + last_id → after_id 参数，最多 10 页
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from urllib.parse import urlencode, urlparse, parse_qs, urlunparse

import httpx

from src.core.logging import LogSource, logger

from src.core.agent.api_key_pool import pick_api_key
from src.core.agent.model_capability_inference import infer_model_capabilities
from src.core.agent.model_group_parser import parse_group_name
from src.core.agent.model_response_parsers import (
    parse_anthropic_model_list,
    parse_azure_model_list,
    parse_gemini_model_list,
    parse_openai_model_list,
)
from src.core.agent.provider import ModelEntry, Provider

_MAX_PAGES = 10


class _HttpError(Exception):
    """内部异常：HTTP 状态码 >= 400."""

    def __init__(self, status_code: int, body_preview: str) -> None:
        self.status_code = status_code
        self.body_preview = body_preview
        super().__init__(f"HTTP {status_code}: {body_preview}")


@dataclass
class FetchResult:
    """模型获取结果."""

    success: bool
    models: list[ModelEntry] = field(default_factory=list)
    error_message: str = ""


class ModelFetchService:
    """模型获取服务 — 根据协议类型从供应商 API 拉取模型列表."""

    async def fetch_models(self, provider: Provider) -> FetchResult:
        """获取供应商的可用模型列表.

        流程:
        1. 检查 api_key_ref 是否为空 → 立即返回错误
        2. 构建请求 URL 和认证头
        3. 发送 GET 请求 (30 秒超时)，支持分页
        4. 解析响应 JSON 为 model_id 列表
        5. 将 model_id 转换为 ModelEntry (推断能力 + 解析分组)
        6. 返回 FetchResult

        错误处理:
        - HTTP 状态码 >= 400: 返回失败 FetchResult
        - 网络超时 (30s): 返回超时错误
        - 连接错误 (DNS/连接拒绝/TLS): 返回连接错误
        - JSON 解析失败: 返回空模型列表 + 记录错误日志
        - 响应缺少预期字段: 返回空模型列表 + 记录警告日志

        分页:
        - Gemini: nextPageToken → pageToken 参数，最多 10 页
        - Anthropic: has_more + last_id → after_id 参数，最多 10 页

        Args:
            provider: 供应商配置对象.

        Returns:
            FetchResult 包含成功/失败状态和模型列表.
        """
        # 1. API 密钥未配置检查
        if not provider.api_key_ref or not provider.api_key_ref.strip():
            return FetchResult(
                success=False,
                error_message="API 密钥未配置，请先设置 api_key_ref",
            )

        # 2. 构建 URL 和 Headers
        url = self._build_fetch_url(provider)
        headers = self._build_fetch_headers(provider)

        # 3. 发送 GET 请求（带错误处理和分页）
        try:
            async with httpx.AsyncClient(timeout=30.0) as client:
                if provider.protocol_type == "gemini":
                    model_ids = await self._fetch_gemini_paginated(
                        client, url, headers, provider
                    )
                elif provider.protocol_type == "anthropic":
                    model_ids = await self._fetch_anthropic_paginated(
                        client, url, headers, provider
                    )
                else:
                    model_ids = await self._fetch_single_page(
                        client, url, headers, provider
                    )
        except httpx.TimeoutException:
            return FetchResult(
                success=False,
                error_message="请求超时（30秒）",
            )
        except httpx.ConnectError as exc:
            error_desc = str(exc)
            return FetchResult(
                success=False,
                error_message=f"连接错误: {error_desc}",
            )
        except _HttpError as exc:
            return FetchResult(
                success=False,
                error_message=f"HTTP 错误 {exc.status_code}: {exc.body_preview}",
            )

        # model_ids 为 None 表示内部已处理错误并返回空列表
        if model_ids is None:
            return FetchResult(success=True, models=[])

        # 5. 转换为 ModelEntry 列表
        models = self._build_model_entries(model_ids)

        # 6. 返回成功结果
        return FetchResult(success=True, models=models)

    async def _fetch_single_page(
        self,
        client: httpx.AsyncClient,
        url: str,
        headers: dict[str, str],
        provider: Provider,
    ) -> list[str] | None:
        """发送单页请求并解析响应.

        Returns:
            model_id 列表，或 None 表示解析失败（已记录日志）.

        Raises:
            httpx.TimeoutException: 请求超时.
            httpx.ConnectError: 连接错误.
        """
        response = await client.get(url, headers=headers)

        # 检查 HTTP 错误
        if response.status_code >= 400:
            raise _HttpError(response.status_code, response.text[:200])

        # 解析 JSON
        data = self._parse_json_response(response, provider)
        if data is None:
            return None

        # 解析模型 ID
        model_ids = self._parse_response(provider.protocol_type, data)

        # 检查是否缺少预期字段（解析结果为空且响应体非空）
        if not model_ids:
            self._check_missing_expected_field(provider, data)

        return model_ids

    async def _fetch_gemini_paginated(
        self,
        client: httpx.AsyncClient,
        base_url: str,
        headers: dict[str, str],
        provider: Provider,
    ) -> list[str] | None:
        """Gemini 分页获取：使用 nextPageToken 进行分页.

        最多请求 10 页，合并所有页的模型 ID。

        Returns:
            合并后的 model_id 列表，或 None 表示解析失败.

        Raises:
            httpx.TimeoutException: 请求超时.
            httpx.ConnectError: 连接错误.
        """
        all_model_ids: list[str] = []
        current_url = base_url

        for page in range(_MAX_PAGES):
            response = await client.get(current_url, headers=headers)

            # 检查 HTTP 错误
            if response.status_code >= 400:
                raise _HttpError(response.status_code, response.text[:200])

            # 解析 JSON
            data = self._parse_json_response(response, provider)
            if data is None:
                return None if not all_model_ids else all_model_ids

            # 解析当前页模型 ID
            page_ids = self._parse_response(provider.protocol_type, data)
            all_model_ids.extend(page_ids)

            # 检查是否有下一页
            next_page_token = data.get("nextPageToken")
            if not next_page_token:
                break

            # 构建下一页 URL
            current_url = self._append_query_param(base_url, "pageToken", next_page_token)

        # 如果所有页都没有模型，检查缺少预期字段
        if not all_model_ids and data is not None:
            self._check_missing_expected_field(provider, data)

        return all_model_ids

    async def _fetch_anthropic_paginated(
        self,
        client: httpx.AsyncClient,
        base_url: str,
        headers: dict[str, str],
        provider: Provider,
    ) -> list[str] | None:
        """Anthropic 分页获取：使用 has_more + last_id 进行分页.

        最多请求 10 页，合并所有页的模型 ID。

        Returns:
            合并后的 model_id 列表，或 None 表示解析失败.

        Raises:
            httpx.TimeoutException: 请求超时.
            httpx.ConnectError: 连接错误.
        """
        all_model_ids: list[str] = []
        current_url = base_url

        for page in range(_MAX_PAGES):
            response = await client.get(current_url, headers=headers)

            # 检查 HTTP 错误
            if response.status_code >= 400:
                raise _HttpError(response.status_code, response.text[:200])

            # 解析 JSON
            data = self._parse_json_response(response, provider)
            if data is None:
                return None if not all_model_ids else all_model_ids

            # 解析当前页模型 ID
            page_ids = self._parse_response(provider.protocol_type, data)
            all_model_ids.extend(page_ids)

            # 检查是否有更多页
            has_more = data.get("has_more", False)
            if not has_more:
                break

            # 获取 last_id 用于下一页请求
            last_id = data.get("last_id")
            if not last_id:
                break

            # 构建下一页 URL
            current_url = self._append_query_param(base_url, "after_id", last_id)

        # 如果所有页都没有模型，检查缺少预期字段
        if not all_model_ids and data is not None:
            self._check_missing_expected_field(provider, data)

        return all_model_ids

    def _parse_json_response(
        self, response: httpx.Response, provider: Provider
    ) -> dict | None:
        """解析 HTTP 响应体为 JSON 字典.

        解析失败时记录错误日志并返回 None。

        Args:
            response: HTTP 响应对象.
            provider: 供应商配置对象.

        Returns:
            解析后的字典，或 None 表示解析失败.
        """
        try:
            return response.json()
        except (json.JSONDecodeError, ValueError):
            raw_text = response.text[:200]
            logger.error(
                f"供应商 {provider.name} 响应 JSON 解析失败，原始响应前 200 字符: {raw_text}",
            )
            return None

    def _check_missing_expected_field(self, provider: Provider, data: dict) -> None:
        """检查响应是否缺少预期的数组字段，记录警告日志.

        Args:
            provider: 供应商配置对象.
            data: 解析后的 JSON 字典.
        """
        # 确定预期字段
        if provider.protocol_type == "gemini":
            expected_field = "models"
        else:
            expected_field = "data"

        # 如果预期字段不存在或不是列表，记录警告
        field_value = data.get(expected_field)
        if not isinstance(field_value, list):
            top_keys = list(data.keys())
            logger.warning(
                f"供应商 {provider.name} 响应缺少预期字段 '{expected_field}'，顶层键列表: {top_keys}",
            )

    @staticmethod
    def _append_query_param(url: str, param_name: str, param_value: str) -> str:
        """向 URL 追加查询参数.

        如果 URL 已有查询参数则追加，否则添加 '?' 开头的参数。

        Args:
            url: 原始 URL.
            param_name: 参数名.
            param_value: 参数值.

        Returns:
            追加参数后的 URL.
        """
        parsed = urlparse(url)
        existing_params = parse_qs(parsed.query)
        existing_params[param_name] = [param_value]
        new_query = urlencode(existing_params, doseq=True)
        new_parsed = parsed._replace(query=new_query)
        return urlunparse(new_parsed)

    def _build_fetch_url(self, provider: Provider) -> str:
        """根据协议类型构建模型列表 API URL.

        URL 构建规则:
        - OpenAI: "{api_base_url}/models" (去除尾部斜杠)
        - Anthropic: "{api_base_url}/models" (去除尾部斜杠)
        - Gemini: "{api_base_url}/models" (去除尾部斜杠)
        - Azure: "{resource_endpoint}/openai/models?api-version={api_version}"

        Args:
            provider: 供应商配置对象.

        Returns:
            完整的 API URL 字符串.
        """
        if provider.protocol_type == "azure":
            if provider.azure_config is None:
                # 回退: 使用 api_base_url
                base = str(provider.api_base_url).rstrip("/")
                return f"{base}/openai/models?api-version=2024-02-01"
            endpoint = provider.azure_config.resource_endpoint.rstrip("/")
            api_version = provider.azure_config.api_version
            return f"{endpoint}/openai/models?api-version={api_version}"

        # OpenAI / Anthropic / Gemini 统一格式
        base = str(provider.api_base_url).rstrip("/")
        return f"{base}/models"

    def _build_fetch_headers(self, provider: Provider) -> dict[str, str]:
        """构建获取请求的认证头.

        认证头规则:
        - OpenAI: Authorization: Bearer {api_key}
        - Anthropic: x-api-key: {api_key}, anthropic-version: 2023-06-01
        - Gemini: x-goog-api-key: {api_key}
        - Azure: api-key: {api_key}

        Args:
            provider: 供应商配置对象.

        Returns:
            请求头字典.
        """
        api_key = pick_api_key(provider.api_key_ref)

        if provider.protocol_type == "anthropic":
            return {
                "x-api-key": api_key,
                "anthropic-version": "2023-06-01",
            }
        elif provider.protocol_type == "gemini":
            return {
                "x-goog-api-key": api_key,
            }
        elif provider.protocol_type == "azure":
            return {
                "api-key": api_key,
            }
        else:
            # OpenAI (default)
            return {
                "Authorization": f"Bearer {api_key}",
            }

    def _parse_response(self, protocol_type: str, data: dict) -> list[str]:
        """解析 API 响应为 model_id 列表.

        根据协议类型调用对应的解析函数。

        Args:
            protocol_type: 协议类型字符串.
            data: API 响应 JSON 字典.

        Returns:
            model_id 字符串列表.
        """
        if protocol_type == "anthropic":
            return parse_anthropic_model_list(data)
        elif protocol_type == "gemini":
            return parse_gemini_model_list(data)
        elif protocol_type == "azure":
            return parse_azure_model_list(data)
        else:
            # OpenAI (default)
            return parse_openai_model_list(data)

    def _build_model_entries(self, model_ids: list[str]) -> list[ModelEntry]:
        """将 model_id 列表转换为 ModelEntry 列表.

        对每个 model_id:
        - 调用 infer_model_capabilities 推断能力标志
        - 调用 parse_group_name 解析分组名称
        - max_tokens 默认设为 4096

        Args:
            model_ids: 模型 ID 字符串列表.

        Returns:
            ModelEntry 对象列表.
        """
        entries: list[ModelEntry] = []
        for model_id in model_ids:
            capabilities = infer_model_capabilities(model_id)
            group_name = parse_group_name(model_id)
            entry = ModelEntry(
                model_id=model_id,
                display_name=model_id,
                group_name=group_name,
                max_tokens=4096,
                supports_vision=capabilities["supports_vision"],
                supports_reasoning=capabilities["supports_reasoning"],
                supports_tools=capabilities["supports_tools"],
                supports_embedding=capabilities["supports_embedding"],
                supports_rerank=capabilities["supports_rerank"],
            )
            entries.append(entry)
        return entries

    def merge_with_existing(
        self,
        fetched_model_ids: list[str],
        existing_models: list[ModelEntry],
    ) -> list[ModelEntry]:
        """将获取到的模型 ID 列表与已有模型列表合并.

        对于已存在的模型（model_id 匹配），保留其手动覆盖的能力标志值。
        对于新模型，使用启发式推断。

        Args:
            fetched_model_ids: 从 API 获取到的模型 ID 列表.
            existing_models: 供应商当前已有的模型列表.

        Returns:
            合并后的 ModelEntry 列表.
        """
        # 1. 构建已有模型的查找字典: {model_id: ModelEntry}
        existing_lookup: dict[str, ModelEntry] = {
            model.model_id: model for model in existing_models
        }

        # 2. 遍历获取到的模型 ID，合并或新建
        merged: list[ModelEntry] = []
        for model_id in fetched_model_ids:
            if model_id in existing_lookup:
                # 已存在 → 保留现有 ModelEntry（含手动覆盖的能力标志）
                merged.append(existing_lookup[model_id])
            else:
                # 新模型 → 使用启发式推断创建新 ModelEntry
                capabilities = infer_model_capabilities(model_id)
                group_name = parse_group_name(model_id)
                entry = ModelEntry(
                    model_id=model_id,
                    display_name=model_id,
                    group_name=group_name,
                    max_tokens=4096,
                    supports_vision=capabilities["supports_vision"],
                    supports_reasoning=capabilities["supports_reasoning"],
                    supports_tools=capabilities["supports_tools"],
                    supports_embedding=capabilities["supports_embedding"],
                    supports_rerank=capabilities["supports_rerank"],
                )
                merged.append(entry)

        return merged

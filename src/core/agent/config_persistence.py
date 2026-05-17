# -*- coding: utf-8 -*-
"""Agent 配置持久化模块.

负责将 Provider 配置, Agent 定义和活跃模型选择持久化到本地 JSON 文件, 
并在应用启动时从文件恢复配置状态. 

容错策略: 
- 文件不存在: 使用默认配置, 记录 warning 日志
- 文件损坏 (无法解析 JSON 或 pydantic 验证失败) : 重命名为 .bak, 使用默认配置, 记录 warning 日志
- 写入 I/O 失败: 记录 error 日志, 不抛出异常
- 缺少 protocol_type 字段: 默认为 "openai"
- 无法识别的 protocol_type 值: 默认为 "openai", 记录 warning 日志
- 未知字段: 忽略, 不报错 (由 Provider model_config extra="ignore" 处理) 
"""

from __future__ import annotations

import json
from pathlib import Path

from pydantic import BaseModel, ValidationError as PydanticValidationError

from src.core.logging import LogSource, logger

from src.core.agent.agent_def import AgentDefinition
from src.core.agent.provider import Provider

# Valid protocol_type values recognized by the system
_VALID_PROTOCOL_TYPES = frozenset({"openai", "anthropic", "gemini", "azure"})


def _normalize_provider_data(provider_data: dict) -> dict:
    """Pre-process a raw provider dict for backward compatibility.

    - Defaults protocol_type to "openai" if missing.
    - Defaults protocol_type to "openai" and logs warning if unrecognized.
    - Preserves azure_config sub-fields as-is.
    - Unknown fields are left in the dict (Pydantic extra="ignore" handles them).

    Args:
        provider_data: Raw provider dictionary from JSON.

    Returns:
        Normalized provider dictionary ready for Pydantic validation.
    """
    data = dict(provider_data)  # shallow copy to avoid mutating original

    if "protocol_type" not in data:
        data["protocol_type"] = "openai"
    else:
        protocol_type = data["protocol_type"]
        if protocol_type not in _VALID_PROTOCOL_TYPES:
            logger.warning(
                f"无法识别的 protocol_type 值 '{protocol_type}'（provider_id={data.get('provider_id', '<unknown>')}），"
                "将默认使用 'openai'",
            )
            data["protocol_type"] = "openai"

    return data


class ConfigData(BaseModel):
    """Agent 配置数据模型.

    Attributes:
        providers: 已注册的 Provider 列表.
        active_provider_id: 当前活跃的 Provider ID, 未设置时为 None.
        active_model_id: 当前活跃的模型 ID, 未设置时为 None.
        agents: Agent 定义列表.
        custom_icon_bindings: provider_id → icon_filename 的自定义图标绑定映射.
    """

    providers: list[Provider] = []
    active_provider_id: str | None = None
    active_model_id: str | None = None
    agents: list[AgentDefinition] = []
    custom_icon_bindings: dict[str, str] = {}


class ConfigPersistence:
    """配置持久化管理器.

    负责从指定路径加载和保存 Agent 配置文件 (agent_config.json) . 

    Args:
        config_file_path: 配置文件的完整路径.
    """

    def __init__(self, config_file_path: Path) -> None:
        self._config_file_path = config_file_path

    @property
    def config_file_path(self) -> Path:
        """配置文件路径."""
        return self._config_file_path

    def load(self) -> ConfigData:
        """从文件加载配置.

        容错逻辑: 
        1. 文件不存在 → 返回默认 ConfigData, 记录 warning
        2. 文件内容无法解析为 JSON 或 pydantic 验证失败 → 重命名为 .bak, 
           返回默认 ConfigData, 记录 warning
        3. Provider 缺少 protocol_type → 默认为 "openai"
        4. Provider 含有无法识别的 protocol_type → 默认为 "openai", 记录 warning
        5. Provider 含有未知字段 → 忽略 (由 Pydantic extra="ignore" 处理) 

        Returns:
            加载的配置数据, 或在容错情况下返回默认配置.
        """
        if not self._config_file_path.exists():
            logger.warning(
                f"配置文件不存在: {self._config_file_path}，使用默认配置初始化",
            )
            return ConfigData()

        try:
            raw_text = self._config_file_path.read_text(encoding="utf-8")
            raw_data = json.loads(raw_text)
            # Pre-process provider configs for backward compatibility
            if "providers" in raw_data and isinstance(raw_data["providers"], list):
                raw_data["providers"] = [
                    _normalize_provider_data(p)
                    for p in raw_data["providers"]
                    if isinstance(p, dict)
                ]
            return ConfigData.model_validate(raw_data)
        except (json.JSONDecodeError, PydanticValidationError, UnicodeDecodeError) as exc:
            logger.warning(
                f"配置文件损坏或验证失败: {self._config_file_path}，错误: {exc}。将重命名为 .bak 并使用默认配置",
            )
            self._rename_to_backup()
            return ConfigData()

    def save(self, config: ConfigData) -> None:
        """原子写入：先写 .tmp 文件，成功后 rename 覆盖目标文件.

        使用临时文件 + Path.replace() 策略确保写入中断时不会损坏原配置文件.
        如果写入过程中发生 I/O 错误, 记录 error 日志但不抛出异常.

        Args:
            config: 要保存的配置数据.
        """
        try:
            self._config_file_path.parent.mkdir(parents=True, exist_ok=True)
            tmp_path = self._config_file_path.with_suffix(".json.tmp")
            json_str = config.model_dump_json(indent=2)
            tmp_path.write_text(json_str, encoding="utf-8")
            tmp_path.replace(self._config_file_path)  # atomic on POSIX/NTFS
        except OSError as exc:
            logger.error(
                f"配置文件写入失败: {self._config_file_path}，错误: {exc}",
            )

    def _rename_to_backup(self) -> None:
        """将损坏的配置文件重命名为 .bak 后缀.

        如果 .bak 文件已存在, 会被覆盖. 
        如果重命名操作本身失败, 记录 warning 但不抛出异常. 
        """
        backup_path = self._config_file_path.with_suffix(
            self._config_file_path.suffix + ".bak"
        )
        try:
            self._config_file_path.replace(backup_path)
            logger.warning(f"已将损坏的配置文件重命名为: {backup_path}")
        except OSError as exc:
            logger.warning(
                f"无法重命名损坏的配置文件: {self._config_file_path}，错误: {exc}",
            )

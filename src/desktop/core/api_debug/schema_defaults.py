# -*- coding: utf-8 -*-
"""根据 WebUI Schema 生成默认请求示例。"""

# 标准库导入
from typing import Any


class SchemaDefaultGenerator:
    """严格按 WebUI 既有顺序生成默认值。"""

    SIMPLE_SCHEMA_KEYS = ("const", "default", "enum", "anyOf", "oneOf", "type", "properties")

    def build_default(self, payload_schema: Any, payload_example: Any = None) -> Any:
        """优先使用 payloadExample，否则按 schema 递归生成默认值。"""
        if payload_example is not None:
            return payload_example
        return self._from_schema(payload_schema)

    def _from_schema(self, schema: Any) -> Any:
        if not isinstance(schema, dict):
            return {}

        if "const" in schema:
            return schema["const"]

        if "default" in schema:
            return schema["default"]

        enum_values = schema.get("enum")
        if isinstance(enum_values, list) and enum_values:
            return enum_values[0]

        for union_key in ("anyOf", "oneOf"):
            variants = schema.get(union_key)
            if isinstance(variants, list) and variants:
                for variant in variants:
                    candidate = self._from_schema(variant)
                    if candidate not in ({}, []):
                        return candidate
                return self._from_schema(variants[0])

        schema_type = schema.get("type")
        if schema_type == "object":
            properties = schema.get("properties", {})
            if not isinstance(properties, dict):
                return {}
            return {str(key): self._from_schema(value) for key, value in properties.items()}

        if schema_type == "array":
            return []

        if schema_type == "string":
            return ""

        if schema_type in {"number", "integer"}:
            return 0

        if schema_type == "boolean":
            return False

        properties = schema.get("properties")
        if isinstance(properties, dict):
            return {str(key): self._from_schema(value) for key, value in properties.items()}

        return {}

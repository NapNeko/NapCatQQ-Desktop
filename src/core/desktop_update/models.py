# -*- coding: utf-8 -*-
"""Desktop 更新领域模型。"""

from pydantic import BaseModel


class DesktopUpdateMigration(BaseModel):
    """Desktop 版本区间迁移规则。"""

    id: str
    from_min: str | None = None
    from_max: str | None = None
    to_version: str | None = None
    to_min: str | None = None
    to_max: str | None = None
    strategy: str = "remote_script"
    script_url: str | None = None
    summary: str | None = None

    def matches(self, local_version: str | None, remote_version: str | None) -> bool:
        """判断当前规则是否命中本地与目标版本组合。"""
        from src.core.desktop_update.planner import _compare_versions, _version_in_range

        if not local_version or not remote_version:
            return False

        if self.strategy == "remote_script" and not self.script_url:
            return False

        if not _version_in_range(local_version, self.from_min, self.from_max):
            return False

        if self.to_version:
            return _compare_versions(remote_version, self.to_version) == 0

        return _version_in_range(remote_version, self.to_min, self.to_max)


class DesktopUpdateManifest(BaseModel):
    """Desktop 远端升级策略清单。"""

    schema_version: int = 2
    min_auto_update_version: str | None = None
    migrations: list[DesktopUpdateMigration] = []


class DesktopUpdatePlan(BaseModel):
    """Desktop 更新决策结果。"""

    kind: str
    summary: str | None = None
    min_auto_update_version: str | None = None
    migration: DesktopUpdateMigration | None = None

    def requires_remote_script(self) -> bool:
        """当前计划是否需要远端迁移脚本。"""
        return bool(
            self.kind == "migration"
            and self.migration is not None
            and self.migration.strategy == "remote_script"
            and self.migration.script_url
        )

    def blocks_update(self) -> bool:
        """当前计划是否阻止自动更新。"""
        return self.kind == "unsupported"

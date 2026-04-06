# -*- coding: utf-8 -*-

from pathlib import Path

import script.utils.generate_changelog_ai as changelog_ai


def test_generate_fallback_changelog_uses_release_note_sections() -> None:
    result = changelog_ai.generate_fallback_changelog(
        "v2.0.18",
        "v2.0.17",
        [
            "feat: 新增安装提示",
            "fix: 修复下载异常",
            "docs: 更新说明文档",
        ],
    )

    assert "## ✨ 新增功能" in result
    assert "## 🐛 修复功能" in result
    assert "## 🧰 其他更新" in result
    assert "- feat: 新增安装提示" in result
    assert "- fix: 修复下载异常" in result


def test_render_preview_document_updates_title_and_marker_block(monkeypatch, tmp_path: Path) -> None:
    project_root = tmp_path / "repo"
    docs_dir = project_root / "docs"
    docs_dir.mkdir(parents=True)
    (docs_dir / "CHANGELOG.md").write_text(
        """# 🚀 NapCatQQ Desktop 更新日志（v2.0.17）

## Tips
- 手动说明

<!-- BEGIN AUTO RELEASE NOTES -->
旧内容
<!-- END AUTO RELEASE NOTES -->

## ⚠️ 重要提醒
- 保留内容
""",
        encoding="utf-8",
    )
    monkeypatch.setattr(changelog_ai, "PROJECT_ROOT", project_root)

    result = changelog_ai.render_preview_document("v2.0.18", "## 🐛 修复功能\n- 修复一个问题")

    assert "# 🚀 NapCatQQ Desktop 更新日志（v2.0.18）" in result
    assert "旧内容" not in result
    assert "## 🐛 修复功能\n- 修复一个问题" in result
    assert "## ⚠️ 重要提醒\n- 保留内容" in result


def test_apply_to_docs_changelog_syncs_semver_title(monkeypatch, tmp_path: Path) -> None:
    project_root = tmp_path / "repo"
    docs_dir = project_root / "docs"
    docs_dir.mkdir(parents=True)
    changelog_path = docs_dir / "CHANGELOG.md"
    changelog_path.write_text(
        """# 🚀 NapCatQQ Desktop 更新日志（v2.0.17）

## Tips
- 手动说明

<!-- BEGIN AUTO RELEASE NOTES -->
旧内容
<!-- END AUTO RELEASE NOTES -->
""",
        encoding="utf-8",
    )
    monkeypatch.setattr(changelog_ai, "PROJECT_ROOT", project_root)

    changed = changelog_ai.apply_to_docs_changelog("2.0.18", "## ✨ 新增功能\n- 新增一个功能")

    assert changed is True
    content = changelog_path.read_text(encoding="utf-8")
    assert "# 🚀 NapCatQQ Desktop 更新日志（v2.0.18）" in content
    assert "## ✨ 新增功能\n- 新增一个功能" in content
    assert "旧内容" not in content


def test_apply_to_docs_changelog_skips_non_semver(monkeypatch, tmp_path: Path) -> None:
    project_root = tmp_path / "repo"
    docs_dir = project_root / "docs"
    docs_dir.mkdir(parents=True)
    changelog_path = docs_dir / "CHANGELOG.md"
    changelog_path.write_text("原内容\n", encoding="utf-8")
    monkeypatch.setattr(changelog_ai, "PROJECT_ROOT", project_root)

    changed = changelog_ai.apply_to_docs_changelog("HEAD", "## 🧰 其他更新\n- 内容")

    assert changed is False
    assert changelog_path.read_text(encoding="utf-8") == "原内容\n"

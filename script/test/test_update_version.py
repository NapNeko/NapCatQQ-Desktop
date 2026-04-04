# -*- coding: utf-8 -*-

# 标准库导入
import subprocess
from pathlib import Path

# 项目内模块导入
from script.utils.release_helpers import (
    AUTO_RELEASE_NOTES_BEGIN,
    AUTO_RELEASE_NOTES_END,
    CommitEntry,
    categorize_commits,
    parse_version,
    perform_release,
    render_auto_release_notes,
    render_changelog_document,
    resolve_previous_tag,
    sync_release_metadata,
)


def test_parse_version_accepts_plain_and_tagged_values() -> None:
    info = parse_version("v2.0.17")
    assert info.version == "2.0.17"
    assert info.tag == "v2.0.17"
    assert parse_version("2.0.17").tag == "v2.0.17"


def test_resolve_previous_tag_skips_current_or_newer_tags(tmp_path: Path) -> None:
    repo = _init_test_repo(tmp_path)
    (repo / "history.txt").write_text("init\n", encoding="utf-8")
    _commit_all(repo, "feat: 初始化项目")
    (repo / "history.txt").write_text("v2.0.15\n", encoding="utf-8")
    _git(repo, "tag", "v2.0.15")
    _commit_all(repo, "fix: 修复旧问题")
    (repo / "history.txt").write_text("v2.0.16\n", encoding="utf-8")
    _git(repo, "tag", "v2.0.16")
    _commit_all(repo, "feat: 当前版本新功能")

    assert resolve_previous_tag("v2.0.17", root=repo) == "v2.0.16"


def test_categorize_commits_filters_release_metadata_and_keeps_other_updates() -> None:
    categories = categorize_commits(
        [
            CommitEntry("1", "feat(bot): 实现 Bot 自动启动功能"),
            CommitEntry("2", "fix(runtime): 修复登录状态检查异常未捕获导致崩溃"),
            CommitEntry("3", "chore(resource): 更新 Qt 资源版本至 6.10.2"),
            CommitEntry("4", "chore(release): 发布 v2.0.17"),
            CommitEntry("5", "chore: release v2.0.17"),
            CommitEntry("6", "chore: 更新 napcatqq-desktop 版本至 2.0.17"),
            CommitEntry("7", "Merge branch 'master'"),
            CommitEntry("8", "更新 README"),
        ]
    )

    assert categories["feat"] == ["实现 Bot 自动启动功能"]
    assert categories["fix"] == ["修复登录状态检查异常未捕获导致崩溃"]
    assert categories["other"] == ["更新 Qt 资源版本至 6.10.2", "更新 README"]


def test_render_auto_release_notes_includes_other_section() -> None:
    notes = render_auto_release_notes(
        {
            "feat": ["实现 Bot 自动启动功能"],
            "fix": [],
            "perf": [],
            "other": ["更新 README"],
        }
    )

    assert "## ✨ 新增功能" in notes
    assert "## 🧰 其他更新" in notes
    assert "- 更新 README" in notes


def test_render_changelog_document_replaces_only_marker_block() -> None:
    existing = """# 🚀 NapCatQQ Desktop 更新日志（v2.0.16）

## Tips
- 手动说明

<!-- BEGIN AUTO RELEASE NOTES -->
旧内容
<!-- END AUTO RELEASE NOTES -->

## ⚠️ 重要提醒
- 保留
"""

    updated = render_changelog_document("v2.0.17", "## ✨ 新增功能\n- 新增功能", existing_content=existing)

    assert "# 🚀 NapCatQQ Desktop 更新日志（v2.0.17）" in updated
    assert "旧内容" not in updated
    assert "## ⚠️ 重要提醒\n- 保留" in updated
    assert AUTO_RELEASE_NOTES_BEGIN in updated
    assert AUTO_RELEASE_NOTES_END in updated


def test_sync_release_metadata_updates_files_and_filters_release_commits(tmp_path: Path) -> None:
    repo = _seed_release_repo(tmp_path)
    _git(repo, "tag", "v2.0.16")
    _commit_release_change(repo, "feat(bot): 实现 Bot 自动启动功能")
    _commit_release_change(repo, "fix(runtime): 修复登录状态检查异常未捕获导致崩溃")
    _commit_release_change(repo, "chore(release): 发布 v2.0.17")
    _commit_release_change(repo, "chore: 更新 napcatqq-desktop 版本至 2.0.17")

    def fake_lock(root: Path) -> None:
        lock_path = root / "uv.lock"
        lock_path.write_text(
            lock_path.read_text(encoding="utf-8").replace('version = "2.0.16"', 'version = "2.0.17"', 1),
            encoding="utf-8",
        )

    result = sync_release_metadata("v2.0.17", root=repo, lock_executor=fake_lock)

    assert result.previous_tag == "v2.0.16"
    assert [commit.subject for commit in result.included_commits] == [
        "feat(bot): 实现 Bot 自动启动功能",
        "fix(runtime): 修复登录状态检查异常未捕获导致崩溃",
    ]
    assert 'version = "2.0.17"' in (repo / "pyproject.toml").read_text(encoding="utf-8")
    assert '__version__ = "v2.0.17"' in (repo / "src/core/config/__init__.py").read_text(encoding="utf-8")
    changelog = (repo / "docs/CHANGELOG.md").read_text(encoding="utf-8")
    assert AUTO_RELEASE_NOTES_BEGIN in changelog
    assert "实现 Bot 自动启动功能" in changelog
    assert 'version = "2.0.17"' in (repo / "uv.lock").read_text(encoding="utf-8")


def test_perform_release_creates_single_release_commit_and_tag(tmp_path: Path) -> None:
    repo = _seed_release_repo(tmp_path)
    _git(repo, "tag", "v2.0.16")
    _commit_release_change(repo, "feat(bot): 实现 Bot 自动启动功能")
    _commit_release_change(repo, "fix(runtime): 修复登录状态检查异常未捕获导致崩溃")

    def fake_lock(root: Path) -> None:
        lock_path = root / "uv.lock"
        lock_path.write_text(
            lock_path.read_text(encoding="utf-8").replace('version = "2.0.16"', 'version = "2.0.17"', 1),
            encoding="utf-8",
        )

    result = perform_release("v2.0.17", root=repo, lock_executor=fake_lock)

    assert result.sync.previous_tag == "v2.0.16"
    assert _git(repo, "rev-parse", "HEAD") == result.commit_sha
    assert _git(repo, "rev-list", "-n", "1", "v2.0.17") == result.commit_sha
    assert _git(repo, "log", "--pretty=%s", "-1") == "chore(release): 发布 v2.0.17"
    assert 'version = "2.0.17"' in (repo / "uv.lock").read_text(encoding="utf-8")


def _init_test_repo(tmp_path: Path) -> Path:
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(repo, "init")
    _git(repo, "config", "user.name", "Test User")
    _git(repo, "config", "user.email", "test@example.com")
    return repo


def _seed_release_repo(tmp_path: Path) -> Path:
    repo = _init_test_repo(tmp_path)
    (repo / "docs").mkdir(parents=True, exist_ok=True)
    (repo / "src/core/config").mkdir(parents=True, exist_ok=True)
    (repo / "pyproject.toml").write_text(
        '[project]\nname = "NapCatQQ-Desktop"\nversion = "2.0.16"\n',
        encoding="utf-8",
    )
    (repo / "src/core/config/__init__.py").write_text(
        '__version__ = "v2.0.16"\n',
        encoding="utf-8",
    )
    (repo / "docs/CHANGELOG.md").write_text(
        """# 🚀 NapCatQQ Desktop 更新日志（v2.0.16）

## Tips
- 手动说明

<!-- BEGIN AUTO RELEASE NOTES -->
## 🧰 其他更新
- 旧发布说明
<!-- END AUTO RELEASE NOTES -->

## ⚠️ 重要提醒
- 保留
""",
        encoding="utf-8",
    )
    (repo / "uv.lock").write_text(
        '[[package]]\nname = "napcatqq-desktop"\nversion = "2.0.16"\n',
        encoding="utf-8",
    )
    _commit_all(repo, "chore: init")
    return repo


def _commit_release_change(repo: Path, message: str) -> None:
    marker = repo / "commits.log"
    current = marker.read_text(encoding="utf-8") if marker.exists() else ""
    marker.write_text(current + message + "\n", encoding="utf-8")
    _commit_all(repo, message)


def _commit_all(repo: Path, message: str) -> None:
    _git(repo, "add", ".")
    _git(repo, "commit", "-m", message)


def _git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(f"git {' '.join(args)} failed: {(result.stderr or result.stdout).strip()}")
    return (result.stdout or "").strip()

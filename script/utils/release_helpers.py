# -*- coding: utf-8 -*-

from __future__ import annotations

# 标准库导入
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

REPO_ROOT = Path(__file__).resolve().parents[2]
PYPROJECT_PATH = REPO_ROOT / "pyproject.toml"
INIT_PATH = REPO_ROOT / "src/core/config/__init__.py"
CHANGELOG_PATH = REPO_ROOT / "docs/CHANGELOG.md"
UV_LOCK_PATH = REPO_ROOT / "uv.lock"

AUTO_RELEASE_NOTES_BEGIN = "<!-- BEGIN AUTO RELEASE NOTES -->"
AUTO_RELEASE_NOTES_END = "<!-- END AUTO RELEASE NOTES -->"
CHANGELOG_TITLE_TEMPLATE = "# 🚀 NapCatQQ Desktop 更新日志（v{version}）"
DEFAULT_CHANGELOG_TEMPLATE = """# 🚀 NapCatQQ Desktop 更新日志（v{version}）

## Tips
- v2.0 起为破坏性更新，旧版无法直接更新，请手动下载新版安装包。
- 安装完成后，可在设置页面迁移或重新导入旧版本配置。

<!-- BEGIN AUTO RELEASE NOTES -->
{auto_notes}
<!-- END AUTO RELEASE NOTES -->

## ⚠️ 重要提醒
- 以上发布说明由发布脚本根据 Git 提交自动整理，发布前应人工复核一次。
- 如果遇到问题，请通过 GitHub Issue 反馈。
"""

SEMVER_PATTERN = re.compile(r"^v?(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)$")
CONVENTIONAL_PATTERN = re.compile(r"^(?P<type>[a-zA-Z]+)(?:\([^)]+\))?!?:\s*(?P<message>.+)$")

RELEASE_METADATA_PATTERNS = (
    re.compile(
        r"^chore\(release\):\s*发布 v\d+\.\d+\.\d+$",
        re.IGNORECASE,
    ),
    re.compile(r"^chore(?:\([^)]+\))?:\s*release v\d+\.\d+\.\d+$", re.IGNORECASE),
    re.compile(
        r"^chore(?:\([^)]+\))?:\s*更新 napcatqq-desktop 版本至 \d+\.\d+\.\d+",
        re.IGNORECASE,
    ),
    re.compile(r"^merge(?: branch| pull request)\b", re.IGNORECASE),
)

EMOJI_TYPE_MAP = {
    "✨": "feat",
    "🐛": "fix",
    "⚡": "perf",
    "♻️": "perf",
    "♻": "perf",
}

CONVENTIONAL_TYPE_MAP = {
    "feat": "feat",
    "feature": "feat",
    "fix": "fix",
    "bugfix": "fix",
    "hotfix": "fix",
    "perf": "perf",
    "refactor": "perf",
    "optimize": "perf",
    "improve": "perf",
    "docs": "other",
    "doc": "other",
    "chore": "other",
    "build": "other",
    "ci": "other",
    "test": "other",
    "style": "other",
    "revert": "other",
}

FEAT_PREFIXES = ("新增", "添加", "支持", "实现", "引入")
FIX_PREFIXES = ("修复", "解决", "修正", "避免", "防止")
PERF_PREFIXES = ("优化", "重构", "提升", "改进", "简化")


class ReleaseError(RuntimeError):
    """发布流程错误。"""


@dataclass(frozen=True)
class VersionInfo:
    version: str
    tag: str
    parts: tuple[int, int, int]


@dataclass(frozen=True)
class CommitEntry:
    sha: str
    subject: str


@dataclass(frozen=True)
class SyncResult:
    version: VersionInfo
    previous_tag: str | None
    commits: list[CommitEntry]
    included_commits: list[CommitEntry]
    categories: dict[str, list[str]]
    auto_notes: str


@dataclass(frozen=True)
class ReleaseResult:
    sync: SyncResult
    commit_sha: str
    pushed: bool


def run_process(
    args: list[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    """运行子进程并返回完整结果。"""
    result = subprocess.run(
        args,
        cwd=cwd or REPO_ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if check and result.returncode != 0:
        message = (result.stderr or result.stdout or "").strip()
        raise ReleaseError(f"命令执行失败: {' '.join(args)}\n{message}")
    return result


def run_command(args: list[str], *, cwd: Path | None = None, check: bool = True) -> str:
    """运行命令并返回 stdout。"""
    result = run_process(args, cwd=cwd, check=check)
    return (result.stdout or "").strip()


def parse_version(value: str) -> VersionInfo:
    """解析语义化版本号。"""
    match = SEMVER_PATTERN.fullmatch(value.strip())
    if not match:
        raise ReleaseError(f"无效的版本号格式: {value}")

    version = f"{int(match.group('major'))}.{int(match.group('minor'))}.{int(match.group('patch'))}"
    return VersionInfo(
        version=version,
        tag=f"v{version}",
        parts=(
            int(match.group("major")),
            int(match.group("minor")),
            int(match.group("patch")),
        ),
    )


def get_release_file_paths(root: Path = REPO_ROOT) -> list[Path]:
    """返回发布流程维护的文件列表。"""
    return [
        root / PYPROJECT_PATH.relative_to(REPO_ROOT),
        root / INIT_PATH.relative_to(REPO_ROOT),
        root / CHANGELOG_PATH.relative_to(REPO_ROOT),
        root / UV_LOCK_PATH.relative_to(REPO_ROOT),
    ]


def get_release_file_relpaths() -> list[str]:
    """返回发布文件的仓库相对路径。"""
    return [
        str(PYPROJECT_PATH.relative_to(REPO_ROOT)).replace("\\", "/"),
        str(INIT_PATH.relative_to(REPO_ROOT)).replace("\\", "/"),
        str(CHANGELOG_PATH.relative_to(REPO_ROOT)).replace("\\", "/"),
        str(UV_LOCK_PATH.relative_to(REPO_ROOT)).replace("\\", "/"),
    ]


def tag_exists(tag: str, *, root: Path = REPO_ROOT) -> bool:
    """判断 tag 是否存在。"""
    result = run_command(["git", "tag", "-l", tag], cwd=root, check=False)
    return tag in {item.strip() for item in result.splitlines() if item.strip()}


def list_semver_tags(*, root: Path = REPO_ROOT) -> list[str]:
    """按语义版本倒序列出版本 tag。"""
    output = run_command(
        ["git", "tag", "--list", "v*.*.*", "--sort=-v:refname"],
        cwd=root,
    )
    tags: list[str] = []
    for line in output.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            parse_version(line)
        except ReleaseError:
            continue
        tags.append(line)
    return tags


def resolve_previous_tag(
    version: str | VersionInfo,
    *,
    root: Path = REPO_ROOT,
    from_tag: str | None = None,
) -> str | None:
    """解析上一版本 tag。"""
    version_info = parse_version(version) if isinstance(version, str) else version

    if from_tag:
        normalized = parse_version(from_tag).tag
        if not tag_exists(normalized, root=root):
            raise ReleaseError(f"指定的起始 tag 不存在: {normalized}")
        return normalized

    for tag in list_semver_tags(root=root):
        tag_info = parse_version(tag)
        if tag_info.parts < version_info.parts:
            return tag_info.tag
    return None


def collect_commits(
    previous_tag: str | None,
    *,
    root: Path = REPO_ROOT,
    to_ref: str = "HEAD",
) -> list[CommitEntry]:
    """采集从上一版本到当前引用的提交。"""
    revision_range = f"{previous_tag}..{to_ref}" if previous_tag else to_ref
    output = run_command(
        ["git", "log", "--reverse", "--pretty=format:%H%x1f%s", revision_range],
        cwd=root,
    )

    commits: list[CommitEntry] = []
    for line in output.splitlines():
        if not line.strip():
            continue
        sha, _, subject = line.partition("\x1f")
        commits.append(CommitEntry(sha=sha.strip(), subject=subject.strip()))
    return commits


def is_release_metadata_commit(subject: str) -> bool:
    """判断是否属于发布元数据提交。"""
    stripped = subject.strip()
    return any(pattern.search(stripped) for pattern in RELEASE_METADATA_PATTERNS)


def _classify_subject(subject: str) -> tuple[str | None, str]:
    stripped = subject.strip()
    if not stripped:
        return None, stripped

    if is_release_metadata_commit(stripped):
        return None, stripped

    for emoji, category in EMOJI_TYPE_MAP.items():
        if stripped.startswith(emoji):
            return category, stripped.removeprefix(emoji).strip()

    match = CONVENTIONAL_PATTERN.match(stripped)
    if match:
        commit_type = match.group("type").lower()
        message = match.group("message").strip()
        return CONVENTIONAL_TYPE_MAP.get(commit_type, "other"), message

    if stripped.startswith(FEAT_PREFIXES):
        return "feat", stripped
    if stripped.startswith(FIX_PREFIXES):
        return "fix", stripped
    if stripped.startswith(PERF_PREFIXES):
        return "perf", stripped
    return "other", stripped


def categorize_commits(commits: list[CommitEntry | str]) -> dict[str, list[str]]:
    """将提交分类为新增、修复、优化和其他。"""
    categories: dict[str, list[str]] = {
        "feat": [],
        "fix": [],
        "perf": [],
        "other": [],
    }

    for commit in commits:
        subject = commit.subject if isinstance(commit, CommitEntry) else str(commit)
        category, message = _classify_subject(subject)
        if category is None:
            continue
        if message and message not in categories[category]:
            categories[category].append(message)

    return categories


def render_auto_release_notes(categories: dict[str, list[str]]) -> str:
    """渲染自动生成的发布说明。"""
    sections: list[str] = []
    ordered_sections = (
        ("feat", "## ✨ 新增功能"),
        ("fix", "## 🐛 修复功能"),
        ("perf", "## 🔧 优化功能"),
        ("other", "## 🧰 其他更新"),
    )

    for key, title in ordered_sections:
        items = categories.get(key, [])
        if not items:
            continue
        sections.append(title)
        sections.extend(f"- {item}" for item in items)
        sections.append("")

    if not sections:
        return "## 🧰 其他更新\n- 累积维护更新"

    return "\n".join(sections).strip()


def format_changelog_title(version: str | VersionInfo) -> str:
    """生成 CHANGELOG 标题。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    return CHANGELOG_TITLE_TEMPLATE.format(version=version_info.version)


def render_changelog_document(
    version: str | VersionInfo,
    auto_notes: str,
    *,
    existing_content: str | None = None,
) -> str:
    """渲染 docs/CHANGELOG.md。"""
    title = format_changelog_title(version)
    notes = auto_notes.strip()
    existing = (existing_content or "").replace("\r\n", "\n")

    if AUTO_RELEASE_NOTES_BEGIN in existing and AUTO_RELEASE_NOTES_END in existing:
        before, rest = existing.split(AUTO_RELEASE_NOTES_BEGIN, 1)
        _, after = rest.split(AUTO_RELEASE_NOTES_END, 1)
        before_lines = before.rstrip().splitlines()
        if before_lines and before_lines[0].startswith("# "):
            before_lines[0] = title
        else:
            before_lines.insert(0, title)
        before_text = "\n".join(before_lines).rstrip()
        after_text = after.lstrip("\n")
        return (
            f"{before_text}\n\n"
            f"{AUTO_RELEASE_NOTES_BEGIN}\n"
            f"{notes}\n"
            f"{AUTO_RELEASE_NOTES_END}\n\n"
            f"{after_text.rstrip()}\n"
        )

    version_info = parse_version(version) if isinstance(version, str) else version
    return (
        DEFAULT_CHANGELOG_TEMPLATE.format(
            version=version_info.version,
            auto_notes=notes,
        ).rstrip()
        + "\n"
    )


def update_pyproject_version(version: str | VersionInfo, *, root: Path = REPO_ROOT) -> None:
    """更新 pyproject.toml 的版本号。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    file_path = root / PYPROJECT_PATH.relative_to(REPO_ROOT)
    content = file_path.read_text(encoding="utf-8")
    new_content = re.sub(
        r'(\[project\][\s\S]*?^version\s*=\s*)"[^"]+"',
        rf'\1"{version_info.version}"',
        content,
        flags=re.MULTILINE,
    )
    if new_content == content and f'version = "{version_info.version}"' not in content:
        raise ReleaseError(f"未能更新版本号: {file_path}")
    if new_content != content:
        file_path.write_text(new_content, encoding="utf-8")


def update_init_version(version: str | VersionInfo, *, root: Path = REPO_ROOT) -> None:
    """更新 src/core/config/__init__.py 的版本号。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    file_path = root / INIT_PATH.relative_to(REPO_ROOT)
    content = file_path.read_text(encoding="utf-8")
    new_content = re.sub(
        r'(__version__\s*=\s*)"[^"]+"',
        rf'\1"{version_info.tag}"',
        content,
    )
    if new_content == content and f'__version__ = "{version_info.tag}"' not in content:
        raise ReleaseError(f"未能更新版本号: {file_path}")
    if new_content != content:
        file_path.write_text(new_content, encoding="utf-8")


def update_changelog(version: str | VersionInfo, auto_notes: str, *, root: Path = REPO_ROOT) -> None:
    """更新 docs/CHANGELOG.md。"""
    file_path = root / CHANGELOG_PATH.relative_to(REPO_ROOT)
    existing_content = file_path.read_text(encoding="utf-8") if file_path.exists() else ""
    file_path.write_text(
        render_changelog_document(version, auto_notes, existing_content=existing_content),
        encoding="utf-8",
    )


def execute_uv_lock(root: Path = REPO_ROOT) -> None:
    """执行真实的 uv lock。"""
    run_process(["uv", "lock"], cwd=root, check=True)


def sync_release_metadata(
    version: str | VersionInfo,
    *,
    root: Path = REPO_ROOT,
    from_tag: str | None = None,
    to_ref: str = "HEAD",
    run_lock: bool = True,
    lock_executor: Callable[[Path], None] | None = None,
) -> SyncResult:
    """同步发布相关文件。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    previous_tag = resolve_previous_tag(version_info, root=root, from_tag=from_tag)
    commits = collect_commits(previous_tag, root=root, to_ref=to_ref)
    included_commits = [commit for commit in commits if not is_release_metadata_commit(commit.subject)]
    categories = categorize_commits(included_commits)
    auto_notes = render_auto_release_notes(categories)

    update_pyproject_version(version_info, root=root)
    update_init_version(version_info, root=root)
    update_changelog(version_info, auto_notes, root=root)

    if run_lock:
        (lock_executor or execute_uv_lock)(root)

    return SyncResult(
        version=version_info,
        previous_tag=previous_tag,
        commits=commits,
        included_commits=included_commits,
        categories=categories,
        auto_notes=auto_notes,
    )


def ensure_clean_worktree(*, root: Path = REPO_ROOT) -> None:
    """确保工作区干净，避免混入非发布改动。"""
    status = run_command(["git", "status", "--porcelain"], cwd=root, check=False)
    if status.strip():
        raise ReleaseError("工作区存在未提交改动，请先清理后再执行发布脚本。")


def stage_release_files(*, root: Path = REPO_ROOT) -> None:
    """暂存发布文件。"""
    run_command(["git", "add", *get_release_file_relpaths()], cwd=root)


def ensure_release_changes_staged(*, root: Path = REPO_ROOT) -> None:
    """确保发布文件确实发生了变更。"""
    result = run_process(
        ["git", "diff", "--cached", "--quiet", "--", *get_release_file_relpaths()],
        cwd=root,
        check=False,
    )
    if result.returncode == 0:
        raise ReleaseError("发布文件没有产生新的变更，已停止创建空发布。")
    if result.returncode not in (0, 1):
        message = (result.stderr or result.stdout or "").strip()
        raise ReleaseError(f"检查暂存区失败: {message}")


def create_release_commit(version: str | VersionInfo, *, root: Path = REPO_ROOT) -> str:
    """创建 release commit。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    stage_release_files(root=root)
    ensure_release_changes_staged(root=root)
    run_command(["git", "commit", "-m", f"chore(release): 发布 {version_info.tag}"], cwd=root)
    return run_command(["git", "rev-parse", "HEAD"], cwd=root)


def create_tag(version: str | VersionInfo, *, root: Path = REPO_ROOT) -> str:
    """创建版本 tag。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    if tag_exists(version_info.tag, root=root):
        raise ReleaseError(f"目标 tag 已存在: {version_info.tag}")
    run_command(["git", "tag", version_info.tag], cwd=root)
    return version_info.tag


def push_release(version: str | VersionInfo, *, root: Path = REPO_ROOT) -> None:
    """推送当前分支和 tag。"""
    version_info = parse_version(version) if isinstance(version, str) else version
    branch = run_command(["git", "branch", "--show-current"], cwd=root)
    if not branch:
        raise ReleaseError("当前处于 detached HEAD，无法自动推送分支。")
    run_command(["git", "push", "origin", branch], cwd=root)
    run_command(["git", "push", "origin", version_info.tag], cwd=root)


def perform_release(
    version: str,
    *,
    root: Path = REPO_ROOT,
    from_tag: str | None = None,
    push: bool = False,
    lock_executor: Callable[[Path], None] | None = None,
) -> ReleaseResult:
    """执行完整本地发布流程。"""
    ensure_clean_worktree(root=root)
    version_info = parse_version(version)
    if tag_exists(version_info.tag, root=root):
        raise ReleaseError(f"目标 tag 已存在: {version_info.tag}")

    sync = sync_release_metadata(
        version_info,
        root=root,
        from_tag=from_tag,
        run_lock=True,
        lock_executor=lock_executor,
    )
    commit_sha = create_release_commit(version_info, root=root)
    create_tag(version_info, root=root)
    if push:
        push_release(version_info, root=root)

    return ReleaseResult(sync=sync, commit_sha=commit_sha, pushed=push)

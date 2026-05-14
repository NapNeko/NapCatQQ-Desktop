# -*- coding: utf-8 -*-
"""Property-based tests for file tools path traversal prevention.

Property 16: Path traversal prevention.
Validates: Requirements 12.8

For any file path that resolves to a location outside the designated workspace
directory (including paths with ../ sequences, symlinks, or absolute paths
outside workspace), the file_read tool SHALL return a ToolResult with
is_error=True indicating a path traversal violation.
"""

from __future__ import annotations

import asyncio
import platform
from pathlib import Path

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from src.core.agent.tools.file_tools import FileReadParams, FileReadTool


def _run(coro):
    """运行异步协程的辅助函数."""
    return asyncio.run(coro)


# ---------------------------------------------------------------------------
# Strategies for generating paths that escape the workspace
# ---------------------------------------------------------------------------

# Strategy 1: Paths with ../ sequences (e.g., "../secret", "subdir/../../outside")
_dotdot_prefix = st.sampled_from([
    "../",
    "../../",
    "../../../",
    "../../../../",
    "../../../../../",
])

_dotdot_suffix = st.text(
    alphabet=st.characters(
        whitelist_categories=("L", "N"),
        whitelist_characters="_-.",
    ),
    min_size=1,
    max_size=30,
).filter(lambda s: s.strip() != "" and ".." not in s)

# Simple ../ prefix paths: "../secret", "../../passwd", etc.
_simple_dotdot_paths = st.builds(
    lambda prefix, suffix: prefix + suffix,
    _dotdot_prefix,
    _dotdot_suffix,
)

# Strategy 2: Paths with subdirectory then ../ to escape
# e.g., "subdir/../../outside", "a/b/../../../secret"
_subdir_segments = st.lists(
    st.text(
        alphabet=st.characters(whitelist_categories=("Ll",), min_codepoint=97, max_codepoint=122),
        min_size=1,
        max_size=10,
    ),
    min_size=1,
    max_size=3,
)

_escape_depth = st.integers(min_value=1, max_value=5)


@st.composite
def _subdir_escape_paths(draw):
    """Generate paths like 'subdir/../../outside' that escape via subdirectory."""
    segments = draw(_subdir_segments)
    # Need more ../ than subdirectory depth to escape workspace
    extra_escape = draw(st.integers(min_value=1, max_value=4))
    dotdot_count = len(segments) + extra_escape
    suffix = draw(_dotdot_suffix)

    path = "/".join(segments) + "/" + "../" * dotdot_count + suffix
    return path


# Strategy 3: Absolute paths outside workspace
_is_windows = platform.system() == "Windows"

if _is_windows:
    _absolute_paths_outside = st.sampled_from([
        "C:\\Windows\\system32\\config\\sam",
        "C:\\Users\\Public\\secret.txt",
        "D:\\data\\passwords.txt",
        "C:\\Windows\\System32\\drivers\\etc\\hosts",
        "C:\\Program Files\\secret.exe",
        "C:\\temp\\malicious.bat",
        "C:\\Users\\Administrator\\Desktop\\file.txt",
    ])
else:
    _absolute_paths_outside = st.sampled_from([
        "/etc/passwd",
        "/etc/shadow",
        "/root/.ssh/id_rsa",
        "/var/log/syslog",
        "/tmp/malicious",
        "/home/user/.bashrc",
        "/usr/local/bin/secret",
    ])

# Strategy 4: Paths with multiple ../ to escape deeply nested workspaces
_deep_escape_paths = st.builds(
    lambda n, suffix: "../" * n + suffix,
    st.integers(min_value=2, max_value=10),
    _dotdot_suffix,
)

# Combined strategy: all path traversal variants
_traversal_paths = st.one_of(
    _simple_dotdot_paths,
    _subdir_escape_paths(),
    _absolute_paths_outside,
    _deep_escape_paths,
)


# ---------------------------------------------------------------------------
# Property Test
# ---------------------------------------------------------------------------


class TestPathTraversalPrevention:
    """Property 16: Path traversal prevention.

    **Validates: Requirements 12.8**

    For any file path that resolves to a location outside the designated workspace
    directory (including paths with ../ sequences, symlinks, or absolute paths
    outside workspace), the file_read tool SHALL return a ToolResult with
    is_error=True indicating a path traversal violation.
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(path=_simple_dotdot_paths)
    def test_dotdot_prefix_paths_blocked(self, path: str, tmp_path_factory) -> None:
        """Paths with ../ prefix sequences are detected as traversal violations.

        **Validates: Requirements 12.8**
        """
        workspace = tmp_path_factory.mktemp("ws")
        tool = FileReadTool(workspace_dir=workspace)
        params = FileReadParams(path=path)
        result = _run(tool.execute(params))

        assert result.is_error is True, (
            f"Expected is_error=True for path '{path}', got is_error=False"
        )
        assert "traversal" in result.output.lower(), (
            f"Expected 'traversal' in output for path '{path}', got: {result.output}"
        )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(path=_subdir_escape_paths())
    def test_subdir_escape_paths_blocked(self, path: str, tmp_path_factory) -> None:
        """Paths that traverse through subdirectories to escape are blocked.

        **Validates: Requirements 12.8**
        """
        workspace = tmp_path_factory.mktemp("ws")
        tool = FileReadTool(workspace_dir=workspace)
        params = FileReadParams(path=path)
        result = _run(tool.execute(params))

        assert result.is_error is True, (
            f"Expected is_error=True for path '{path}', got is_error=False"
        )
        assert "traversal" in result.output.lower(), (
            f"Expected 'traversal' in output for path '{path}', got: {result.output}"
        )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(path=_absolute_paths_outside)
    def test_absolute_paths_outside_workspace_blocked(
        self, path: str, tmp_path_factory
    ) -> None:
        """Absolute paths outside the workspace are detected as traversal violations.

        **Validates: Requirements 12.8**
        """
        workspace = tmp_path_factory.mktemp("ws")
        tool = FileReadTool(workspace_dir=workspace)
        params = FileReadParams(path=path)
        result = _run(tool.execute(params))

        assert result.is_error is True, (
            f"Expected is_error=True for absolute path '{path}', got is_error=False"
        )
        assert "traversal" in result.output.lower(), (
            f"Expected 'traversal' in output for path '{path}', got: {result.output}"
        )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(path=_deep_escape_paths)
    def test_deep_escape_paths_blocked(self, path: str, tmp_path_factory) -> None:
        """Paths with many ../ sequences to escape deeply nested workspaces are blocked.

        **Validates: Requirements 12.8**
        """
        workspace = tmp_path_factory.mktemp("ws")
        tool = FileReadTool(workspace_dir=workspace)
        params = FileReadParams(path=path)
        result = _run(tool.execute(params))

        assert result.is_error is True, (
            f"Expected is_error=True for deep escape path '{path}', got is_error=False"
        )
        assert "traversal" in result.output.lower(), (
            f"Expected 'traversal' in output for path '{path}', got: {result.output}"
        )

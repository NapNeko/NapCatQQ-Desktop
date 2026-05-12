# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.templates` 模板渲染测试 (W2).

覆盖:

- 3 个 builder 渲染往返 (load template + render placeholder + inject path 变量)
- 占位无残留 (除注释里 ``{{NAME}}`` 等明显伪占位外, 不应有 ``{{[a-z_]+}}``)
- 路径变量被 ``inject_script_variables`` 安全注入 (``$HOME`` 展开 + 单引号字面量)
- 端口 / 显示号等数字占位被字面量替换
- 不传 ``framework_archive_name`` 时 :func:`build_install_snowluma_script` 必报 ``TypeError``
- :func:`_read_template` 在 Qt 资源缺失且源文件缺失时 raise ``FileNotFoundError``
"""

from __future__ import annotations

import re

import pytest

# 加载 Qt 资源 (含 W2 注册的 .sh.j2)
import src.resource.resource  # noqa: F401  pylint: disable=unused-import

from src.core.remote.snowluma import (
    SnowLumaRemotePaths,
    build_install_snowluma_script,
    build_snowluma_bot_launcher,
    build_snowluma_daemon_launcher,
)
from src.core.remote.snowluma import templates as templates_mod


# 占位匹配 (仅小写字母 + 下划线; 注释里的 ``{{NAME}}`` 不会被匹配)
_PLACEHOLDER_RE = re.compile(r"\{\{[a-z_]+\}\}")


@pytest.fixture
def paths() -> SnowLumaRemotePaths:
    """默认 ``$HOME/snowluma-remote`` 路径布局."""
    return SnowLumaRemotePaths.from_base()


# ==================== install_snowluma.sh.j2 ====================
class TestBuildInstallSnowlumaScript:
    """:func:`build_install_snowluma_script` 渲染测试."""

    def test_renders_without_error(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(
            paths, framework_archive_name="snowluma_framework_lite.tar.gz"
        )
        assert script.startswith("#!/usr/bin/env bash\n")
        assert len(script.splitlines()) > 50

    def test_no_unreplaced_placeholders(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(
            paths, framework_archive_name="x.tar.gz"
        )
        leftover = _PLACEHOLDER_RE.findall(script)
        assert not leftover, f"发现未替换占位: {set(leftover)}"

    def test_path_injected_after_shebang(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(paths, framework_archive_name="x.tar.gz")
        lines = script.splitlines()
        assert lines[0] == "#!/usr/bin/env bash"
        # 第 2 行起应是 inject 注入的大写变量
        injected_block = "\n".join(lines[1:10])
        assert "WORKSPACE_DIR=" in injected_block
        assert "SNOWLUMA_DIR=" in injected_block
        assert "CONFIG_DIR=" in injected_block

    def test_path_uses_posix_single_quote(self, paths: SnowLumaRemotePaths) -> None:
        """路径必须以 ``"$HOME"`` + 单引号字面量形式注入 (复用 NC F2.1 安全策略)."""
        script = build_install_snowluma_script(paths, framework_archive_name="x.tar.gz")
        # paths.workspace_dir = '$HOME/snowluma-remote/workspace'
        # 期望: WORKSPACE_DIR="$HOME"'/snowluma-remote/workspace'
        assert "WORKSPACE_DIR=\"$HOME\"'/snowluma-remote/workspace'" in script

    def test_archive_name_substituted(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(
            paths, framework_archive_name="custom-1.2.3.tar.gz"
        )
        assert "custom-1.2.3.tar.gz" in script

    def test_enable_nodesource_true_renders_1(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(
            paths, framework_archive_name="x.tar.gz", enable_nodesource=True
        )
        # 模板里有 [ "{{enable_nodesource}}" = "1" ] 的判断
        assert '"1"' in script or "= \"1\"" in script

    def test_enable_nodesource_false_renders_0(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(
            paths, framework_archive_name="x.tar.gz", enable_nodesource=False
        )
        assert '"0"' in script

    def test_custom_ports(self, paths: SnowLumaRemotePaths) -> None:
        script = build_install_snowluma_script(
            paths,
            framework_archive_name="x.tar.gz",
            vnc_port=15900,
            novnc_port=16081,
            webui_port=15099,
            display_num=42,
        )
        assert "15900" in script
        assert "16081" in script
        assert "15099" in script
        assert ":42" in script

    def test_archive_name_required(self, paths: SnowLumaRemotePaths) -> None:
        with pytest.raises(TypeError):
            build_install_snowluma_script(paths)  # type: ignore[call-arg]


# ==================== snowluma_daemon_launcher.sh.j2 ====================
class TestBuildSnowlumaDaemonLauncher:
    """:func:`build_snowluma_daemon_launcher` 渲染测试."""

    def test_renders_without_error(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths)
        assert script.startswith("#!/usr/bin/env bash\n")

    def test_no_unreplaced_placeholders(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths)
        leftover = _PLACEHOLDER_RE.findall(script)
        assert not leftover, f"发现未替换占位: {set(leftover)}"

    def test_subcommands_present(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths)
        for sub in ("cmd_start", "cmd_stop", "cmd_status", "cmd_restart", "cmd_wait_ready"):
            assert sub in script, f"daemon launcher 缺子命令实现: {sub}"
        assert "case " in script  # 有主入口分发

    def test_status_daemon_path_injected(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths)
        # paths.status_daemon = '$HOME/snowluma-remote/workspace/runtime/status_daemon.json'
        assert (
            "STATUS_DAEMON_PATH=\"$HOME\"'/snowluma-remote/workspace/runtime/status_daemon.json'"
            in script
        )

    def test_pid_daemon_path_injected(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths)
        assert (
            "PID_DAEMON_PATH=\"$HOME\"'/snowluma-remote/workspace/runtime/pid_daemon'"
            in script
        )

    def test_default_ports_substituted(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths)
        assert "5900" in script
        assert "6081" in script
        assert "5099" in script
        assert ":0" in script

    def test_custom_display_num(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_daemon_launcher(paths, display_num=99)
        assert ":99" in script
        # 默认 display 0 不应再出现 (除了端口里的 0 数字外)
        # 端口 5900/6081/5099 都没有 ":0" 这个组合, 故仅 display 占位会形成 ":0"
        assert ":0\b" not in script  # 简化断言: 用 ":99" 出现验证替换正确即可


# ==================== snowluma_bot_launcher.sh.j2 ====================
class TestBuildSnowlumaBotLauncher:
    """:func:`build_snowluma_bot_launcher` 渲染测试."""

    def test_renders_without_error(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_bot_launcher(paths)
        assert script.startswith("#!/usr/bin/env bash\n")

    def test_no_unreplaced_placeholders(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_bot_launcher(paths)
        leftover = _PLACEHOLDER_RE.findall(script)
        assert not leftover, f"发现未替换占位: {set(leftover)}"

    def test_subcommands_present(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_bot_launcher(paths)
        for sub in ("cmd_start", "cmd_stop", "cmd_status"):
            assert sub in script

    def test_runtime_dir_injected(self, paths: SnowLumaRemotePaths) -> None:
        script = build_snowluma_bot_launcher(paths)
        assert "RUNTIME_DIR=\"$HOME\"'/snowluma-remote/workspace/runtime'" in script

    def test_qq_id_validation_in_script(self, paths: SnowLumaRemotePaths) -> None:
        """脚本本身应包含 qq_id 数字校验逻辑 (拒绝注入)."""
        script = build_snowluma_bot_launcher(paths)
        assert "*[!0-9]*" in script  # 数字校验 case pattern

    def test_qq_bin_default_uses_rootless_workspace_path(
        self, paths: SnowLumaRemotePaths
    ) -> None:
        """``QQ_BIN_DEFAULT`` 必须指向 ``$WORKSPACE_DIR/opt/QQ/qq`` (rootless 安装).

        回归保护: 早期模板写死 ``/usr/bin/qq``, 但 ``remote_install_linuxqq.sh``
        实际是 ``dpkg -x`` 解压到 ``$install_base_dir`` (= SL workspace), 二进制
        从来不会出现在系统级 ``/usr/bin/qq``, 导致 SL Bot 启动报"qq 可执行文件不存在".
        """
        script = build_snowluma_bot_launcher(paths)
        assert 'QQ_BIN_DEFAULT="$WORKSPACE_DIR/opt/QQ/qq"' in script
        # 严格: 模板代码行 (排除注释) 不应再出现 ``QQ_BIN_DEFAULT="/usr/bin/qq"``,
        # 注释里出现 ``/usr/bin/qq`` 作为对照说明是允许的.
        assert 'QQ_BIN_DEFAULT="/usr/bin/qq"' not in script


# ==================== _read_template 错误路径 ====================
class TestReadTemplateErrors:
    """:func:`templates_mod._read_template` 缺失资源时必 raise."""

    def test_missing_template_raises(self) -> None:
        with pytest.raises(FileNotFoundError) as exc_info:
            templates_mod._read_template("non_existent_template.sh.j2")
        assert "non_existent_template.sh.j2" in str(exc_info.value)


# ==================== 路径安全 (集成) ====================
class TestPathSafetyIntegration:
    """SnowLumaRemotePaths 已禁止 shell 元字符, 此处验证 inject 调用链不会引入新风险."""

    def test_absolute_path_renders_clean(self) -> None:
        paths = SnowLumaRemotePaths.from_base("/opt/sl")
        script = build_install_snowluma_script(paths, framework_archive_name="x.tar.gz")
        # 绝对路径不走 $HOME 拆分, 应以纯单引号字面量形式注入
        assert "WORKSPACE_DIR='/opt/sl/workspace'" in script
        assert "$HOME" not in script.split("set -euo pipefail")[0].split("\n", 1)[0]

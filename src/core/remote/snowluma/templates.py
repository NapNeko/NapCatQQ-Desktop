# -*- coding: utf-8 -*-
"""SnowLuma 远端 shell 脚本模板渲染.

3 份脚本与对应构建器:

| 脚本 (远端落地后名) | 用途 | 构建器 |
| --- | --- | --- |
| ``snowluma_install.sh`` | 一次性部署 (图形栈 + node + LinuxQQ + Framework lite tarball) | :func:`build_install_snowluma_script` |
| ``snowluma_daemon_launcher.sh`` | daemon 启停/状态/wait-ready (Xvfb+fluxbox+x11vnc+websockify+node) | :func:`build_snowluma_daemon_launcher` |
| ``snowluma_bot_launcher.sh`` | 单 Bot 启停 (qq.exe spawn + pid/status 协议) | :func:`build_snowluma_bot_launcher` |

每个构建器读取对应 ``.sh.j2`` 模板 (Qt 资源 ``:/script/remote/snowluma/`` 注册见 W2),
然后调用 :func:`src.core.remote.templates.inject_script_variables` 在 shebang 之后
注入路径变量, 返回完整脚本文本.

模板占位约定: 模板内变量用 ``{{name}}`` 标记 (单纯字符串替换, 不引入 jinja2 依赖,
与 NapCat ``src/core/remote/templates.py`` 渲染模型一致).

参考: ``docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md`` §W1/§W2
"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path

from PySide6.QtCore import QFile, QIODevice

from src.core.remote.templates import inject_script_variables

from .paths import SnowLumaRemotePaths


# ==================== 资源定位 ====================
# 开发态回落根目录 (与 NapCat 模板同模式; 打包后通过 Qt resource 读取)
_SCRIPT_DIR: Path = (
    Path(__file__).resolve().parents[3] / "resource" / "script" / "remote" / "snowluma"
)

# Qt 资源前缀: 复用 NapCat ``prefix="script"`` 节点 (避免新 prefix 增加 .qrc 维护负担).
# NapCat 现状是 prefix 与文件路径都含 ``script``, 故访问路径含双 ``script``;
# SnowLuma 模板放在 ``script/remote/snowluma/`` 子目录, 最终访问路径如下例:
# ``:/script/script/remote/snowluma/install_snowluma.sh.j2``
_QT_RESOURCE_PREFIX: str = ":/script/script/remote/snowluma"

# 模板文件名 (W2 落地)
INSTALL_SCRIPT_TEMPLATE: str = "install_snowluma.sh.j2"
DAEMON_LAUNCHER_TEMPLATE: str = "snowluma_daemon_launcher.sh.j2"
BOT_LAUNCHER_TEMPLATE: str = "snowluma_bot_launcher.sh.j2"


def _read_template(filename: str) -> str:
    """读取模板内容; 优先走 Qt 资源, 失败回落到源仓库文件路径.

    与 :func:`src.core.remote.templates._read_template` 同策略保证开发态/打包态都可用.

    Raises:
        FileNotFoundError: Qt 资源未注册且源文件不存在 (W2 资源未就位时单测会触发).
    """
    resource_path = f"{_QT_RESOURCE_PREFIX}/{filename}"
    resource_file = QFile(resource_path)
    if resource_file.open(QIODevice.OpenModeFlag.ReadOnly | QIODevice.OpenModeFlag.Text):
        try:
            return bytes(resource_file.readAll()).decode("utf-8")  # type: ignore[arg-type]
        finally:
            resource_file.close()

    fallback = _SCRIPT_DIR / filename
    if fallback.is_file():
        return fallback.read_text(encoding="utf-8")
    raise FileNotFoundError(
        f"SnowLuma 模板不存在: 既未在 Qt 资源 ({resource_path}) 注册, "
        f"也未在源仓库 ({fallback}) 找到. W2 阶段需先落 .sh.j2 文件."
    )


def _render_placeholders(template_text: str, variables: Mapping[str, str | int]) -> str:
    """把模板里的 ``{{name}}`` 占位替换为 ``variables[name]``.

    规则:

    - 占位严格匹配 ``{{name}}`` (无空格); 模板侧不允许 ``{{ name }}`` 这类带空格变体,
      与 NapCat 现有模板保持一致, 减少正则歧义.
    - 缺失变量 (模板里有但 ``variables`` 没给) **不**会报错; 占位原样保留, 由远端
      脚本自然 unbound 暴露问题. 这与 NapCat 渲染模式一致.
    - 多余变量 (``variables`` 给了但模板里没用) 静默忽略.

    Args:
        template_text: 包含 ``{{name}}`` 占位的模板原文.
        variables: 变量名 → 值映射; 值会先 ``str()`` 化.

    Returns:
        替换后的脚本文本.
    """
    rendered = template_text
    for key, value in variables.items():
        rendered = rendered.replace(f"{{{{{key}}}}}", str(value))
    return rendered


# ==================== 构建器 ====================
def build_install_snowluma_script(
    paths: SnowLumaRemotePaths,
    *,
    framework_download_url: str,
    framework_archive_name: str,
    enable_nodesource: bool = True,
    vnc_port: int = 5900,
    novnc_port: int = 6081,
    webui_port: int = 5099,
    display_num: int = 0,
) -> str:
    """构建 SnowLuma 一次性部署脚本.

    脚本流程 (与 plan §W2 ``install_snowluma.sh.j2`` 契约对齐):

    1. apt 更新包索引
    2. apt 装图形栈 (xvfb / fluxbox / x11vnc / novnc / websockify / dbus-x11 + 中文字体)
    3. 装 node (3 级 fallback: 已装 ≥ 22 / apt nodejs / nodesource setup_22.x)
    4. 从 GitHub releases 下载 lite tarball 到 ``{workspace_dir}/`` 并解压到
       ``{workspace_dir}/snowluma`` (含 ``index.mjs`` + ``native/`` 等)
    5. 生成 ``vnc.secret`` / ``webui.secret`` (mode 600)
    6. 冒烟验证 (``command -v Xvfb fluxbox ...`` + ``node -v ≥ 22``)

    Args:
        paths: 远端目录布局; 通过 :func:`inject_script_variables` 注入到脚本头部.
        framework_download_url: SnowLuma.Framework lite tarball 的 GitHub releases
            下载地址 (例: ``https://github.com/SnowLuma/SnowLuma/releases/download/v1.7.7/SnowLuma-v1.7.7-linux-x64-lite.tar.gz``).
        framework_archive_name: lite tarball 落地文件名 (远端 ``$WORKSPACE_DIR/`` 下).
        enable_nodesource: ``True`` 允许走 nodesource L3 fallback;
            ``False`` 强制只走 apt nodejs (air-gapped 部署或测试).
        vnc_port / novnc_port / webui_port: 对应远端监听端口, 注入到脚本变量.
        display_num: ``Xvfb :N`` 的 N 值, 默认 ``0``.

    Returns:
        渲染好的完整脚本文本 (可直接通过 SFTP 上传 + ``bash`` 执行).

    Raises:
        FileNotFoundError: W2 资源未就位.
    """
    template = _read_template(INSTALL_SCRIPT_TEMPLATE)
    # 编译期占位 (端口/标志/文件名等不含 shell 元字符的纯量); 路径不在此, 走 inject
    placeholder_vars: dict[str, str | int] = {
        "framework_download_url": framework_download_url,
        "framework_archive_name": framework_archive_name,
        "enable_nodesource": "1" if enable_nodesource else "0",
        "vnc_port": vnc_port,
        "novnc_port": novnc_port,
        "webui_port": webui_port,
        "display_num": display_num,
    }
    rendered = _render_placeholders(template, placeholder_vars)
    # 路径变量统一走 inject (POSIX 单引号字面量, 复用 NC F2.1 安全策略)
    inject_vars: Mapping[str, str | int] = {
        "WORKSPACE_DIR": paths.workspace_dir,
        "SNOWLUMA_DIR": paths.snowluma_framework_dir,
        "CONFIG_DIR": paths.config_dir,
        "RUNTIME_DIR": paths.runtime_dir,
        "LOG_DIR": paths.log_dir,
        "VNC_SECRET_PATH": paths.vnc_secret,
        "WEBUI_SECRET_PATH": paths.webui_secret,
    }
    return inject_script_variables(rendered, inject_vars)


def build_snowluma_daemon_launcher(
    paths: SnowLumaRemotePaths,
    *,
    display_num: int = 0,
    vnc_port: int = 5900,
    novnc_port: int = 6081,
    webui_port: int = 5099,
) -> str:
    """构建 daemon launcher 脚本.

    支持子命令 ``start / stop / status / restart / wait-ready`` (详见 plan §W2 契约).
    Desktop 通过 :class:`SnowLumaLauncherCommands` (W5) 调对应子命令.

    Args:
        paths: 远端目录布局.
        display_num: ``Xvfb :N`` 显示号.
        vnc_port: x11vnc 监听端口.
        novnc_port: websockify (noVNC) 监听端口.
        webui_port: SnowLuma WebUI HTTP 端口 (用于 ``wait-ready`` 探测).

    Returns:
        渲染好的脚本文本.
    """
    template = _read_template(DAEMON_LAUNCHER_TEMPLATE)
    placeholder_vars: dict[str, str | int] = {
        "display_num": display_num,
        "vnc_port": vnc_port,
        "novnc_port": novnc_port,
        "webui_port": webui_port,
    }
    rendered = _render_placeholders(template, placeholder_vars)
    inject_vars: Mapping[str, str | int] = {
        "WORKSPACE_DIR": paths.workspace_dir,
        "SNOWLUMA_DIR": paths.snowluma_framework_dir,
        "RUNTIME_DIR": paths.runtime_dir,
        "LOG_DIR": paths.log_dir,
        "VNC_SECRET_PATH": paths.vnc_secret,
        "PID_DAEMON_PATH": paths.pid_daemon,
        "STATUS_DAEMON_PATH": paths.status_daemon,
        "LOG_DAEMON_PATH": paths.log_daemon,
        "DBUS_ENV_PATH": paths.dbus_env_file,
    }
    return inject_script_variables(rendered, inject_vars)


def build_snowluma_bot_launcher(
    paths: SnowLumaRemotePaths,
    *,
    display_num: int = 0,
) -> str:
    """构建 Bot launcher 脚本.

    支持子命令 ``start <qq_id> / stop <qq_id> / status <qq_id>``.

    Args:
        paths: 远端目录布局.
        display_num: ``DISPLAY=:N`` 让 qq.exe 渲染到 daemon 的 Xvfb 屏.

    Returns:
        渲染好的脚本文本.
    """
    template = _read_template(BOT_LAUNCHER_TEMPLATE)
    placeholder_vars: dict[str, str | int] = {
        "display_num": display_num,
    }
    rendered = _render_placeholders(template, placeholder_vars)
    inject_vars: Mapping[str, str | int] = {
        "WORKSPACE_DIR": paths.workspace_dir,
        "RUNTIME_DIR": paths.runtime_dir,
        "LOG_DIR": paths.log_dir,
    }
    return inject_script_variables(rendered, inject_vars)

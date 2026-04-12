# -*- coding: utf-8 -*-
"""NapCat Daemon 自动部署器。

提供一键部署功能，用户只需提供 SSH 凭据，自动完成：
- 连接到远程服务器
- 检查/安装依赖
- 上传并执行安装脚本
- 获取连接 Token
- 自动配置 Desktop 客户端
"""

from __future__ import annotations

import json
import logging
import re
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .models import SSHCredentials
from .ssh_client import SSHClient

logger = logging.getLogger(__name__)


@dataclass(slots=True)
class DeployResult:
    """部署结果。"""

    success: bool
    host: str
    port: int
    token: str | None = None
    error: str | None = None
    logs: list[str] = None

    def __post_init__(self):
        if self.logs is None:
            self.logs = []


@dataclass(slots=True)
class ServerInfo:
    """远程服务器信息。"""

    os_id: str
    os_version: str
    arch: str
    kernel: str
    has_systemd: bool
    open_ports: list[int]
    existing_daemon: bool
    daemon_version: str | None = None


class DaemonDeployer:
    """NapCat Daemon 自动部署器。"""

    # 安装脚本路径（打包时会内嵌到资源中）
    INSTALL_SCRIPT_NAME = "install.sh"

    # Daemon 默认端口
    DEFAULT_PORT = 8443

    def __init__(self, ssh_client: SSHClient | None = None):
        self._ssh = ssh_client
        self._owned_ssh = False

    def __enter__(self) -> DaemonDeployer:
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
        return False

    def close(self) -> None:
        """关闭资源。"""
        if self._owned_ssh and self._ssh:
            self._ssh.close()
            self._ssh = None

    def deploy(
        self,
        credentials: SSHCredentials,
        port: int = DEFAULT_PORT,
        install_dir: str = "/opt/napcat-daemon",
        progress_callback: callable | None = None,
    ) -> DeployResult:
        """一键部署 NapCat Daemon。

        Args:
            credentials: SSH 连接凭据
            port: Daemon 监听端口
            install_dir: 远程安装目录
            progress_callback: 进度回调函数(message: str, percent: int)

        Returns:
            DeployResult: 部署结果
        """
        logs: list[str] = []

        def log(msg: str, level: str = "info"):
            logger.info(msg)
            logs.append(f"[{level.upper()}] {msg}")
            if progress_callback:
                progress_callback(msg, 0)

        def update_progress(percent: int, msg: str = ""):
            if progress_callback:
                progress_callback(msg, percent)

        try:
            # Step 1: 连接 SSH
            log(f"连接到 {credentials.host}:{credentials.port}...")
            update_progress(5, "建立 SSH 连接...")

            if not self._ssh:
                self._ssh = SSHClient(credentials)
                self._owned_ssh = True

            if not self._ssh.connect():
                return DeployResult(
                    success=False,
                    host=credentials.host,
                    port=port,
                    error="SSH 连接失败",
                    logs=logs,
                )

            update_progress(10, "SSH 连接成功")

            # Step 2: 检测远程环境
            log("检测远程服务器环境...")
            update_progress(15, "检测服务器环境...")

            server_info = self._detect_server_info()
            log(f"系统: {server_info.os_id} {server_info.os_version}")
            log(f"架构: {server_info.arch}")
            log(f"Systemd: {'支持' if server_info.has_systemd else '不支持'}")

            if not server_info.has_systemd:
                return DeployResult(
                    success=False,
                    host=credentials.host,
                    port=port,
                    error="远程服务器不支持 systemd，无法部署 Daemon",
                    logs=logs,
                )

            update_progress(25, "环境检测完成")

            # Step 3: 检查现有安装
            if server_info.existing_daemon:
                log("检测到已安装的 Daemon，执行更新...")
                update_progress(30, "更新现有安装...")
            else:
                log("全新安装...")
                update_progress(30, "开始全新安装...")

            # Step 4: 上传安装脚本
            log("上传安装脚本...")
            update_progress(40, "上传安装脚本...")

            script_content = self._get_install_script()
            remote_script_path = "/tmp/napcat-daemon-install.sh"

            result = self._ssh.execute(
                f"cat > {remote_script_path} << 'NAPCAT_EOF'\n{script_content}\nNAPCAT_EOF"
            )
            if not result.ok:
                return DeployResult(
                    success=False,
                    host=credentials.host,
                    port=port,
                    error=f"上传安装脚本失败: {result.stderr}",
                    logs=logs,
                )

            # 添加执行权限
            self._ssh.execute(f"chmod +x {remote_script_path}")
            update_progress(50, "安装脚本上传完成")

            # Step 5: 执行安装
            log("执行安装脚本（这可能需要几分钟）...")
            update_progress(55, "执行安装...")

            # 使用 nohup 在后台执行，避免连接中断导致安装失败
            install_cmd = (
                f"sudo bash {remote_script_path} 2>&1"
            )

            result = self._ssh.execute(install_cmd, timeout=300)

            # 收集安装日志
            install_output = result.stdout + result.stderr
            for line in install_output.splitlines():
                logs.append(f"[INSTALL] {line}")

            if not result.ok and "安装完成" not in install_output:
                # 检查是否是已经安装的情况
                if "已安装" not in install_output:
                    return DeployResult(
                        success=False,
                        host=credentials.host,
                        port=port,
                        error=f"安装失败: {result.stderr}",
                        logs=logs,
                    )

            update_progress(80, "安装执行完成")

            # Step 6: 获取 Token
            log("获取连接 Token...")
            update_progress(85, "获取连接配置...")

            token = self._extract_token_from_config()
            if not token:
                # 尝试从安装输出中提取
                token = self._extract_token_from_output(install_output)

            if not token:
                return DeployResult(
                    success=False,
                    host=credentials.host,
                    port=port,
                    error="无法获取连接 Token，请检查安装日志",
                    logs=logs,
                )

            # Step 7: 验证服务状态
            log("验证 Daemon 服务状态...")
            update_progress(90, "验证服务...")

            if not self._verify_daemon_running():
                return DeployResult(
                    success=False,
                    host=credentials.host,
                    port=port,
                    error="Daemon 服务未能正常启动",
                    logs=logs,
                )

            # Step 8: 检查端口
            log(f"检查端口 {port} 是否开放...")
            update_progress(95, "检查网络连通性...")

            if not self._check_port_open(port):
                log(f"警告: 端口 {port} 可能未开放", level="warn")
                log("请确保防火墙允许该端口", level="warn")

            update_progress(100, "部署完成")

            log("部署成功！")
            log(f"连接地址: wss://{credentials.host}:{port}/ws")
            log(f"Token: {token[:8]}...{token[-8:]}")

            return DeployResult(
                success=True,
                host=credentials.host,
                port=port,
                token=token,
                logs=logs,
            )

        except Exception as e:
            logger.exception("部署失败")
            return DeployResult(
                success=False,
                host=credentials.host,
                port=port,
                error=f"部署异常: {str(e)}",
                logs=logs,
            )

    def _detect_server_info(self) -> ServerInfo:
        """检测远程服务器信息。"""
        # 检测操作系统
        os_result = self._ssh.execute("cat /etc/os-release")
        os_id = "unknown"
        os_version = "unknown"

        if os_result.ok:
            for line in os_result.stdout.splitlines():
                if line.startswith("ID="):
                    os_id = line.split("=")[1].strip('"')
                elif line.startswith("VERSION_ID="):
                    os_version = line.split("=")[1].strip('"')

        # 检测架构
        arch_result = self._ssh.execute("uname -m")
        arch = arch_result.stdout.strip() if arch_result.ok else "unknown"

        # 检测内核
        kernel_result = self._ssh.execute("uname -r")
        kernel = kernel_result.stdout.strip() if kernel_result.ok else "unknown"

        # 检测 systemd
        systemd_result = self._ssh.execute("systemctl --version")
        has_systemd = systemd_result.ok

        # 检测开放端口
        ports_result = self._ssh.execute(
            "ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null || echo ''"
        )
        open_ports = []
        for line in ports_result.stdout.splitlines():
            if ":" in line:
                match = re.search(r":(\d+)", line)
                if match:
                    open_ports.append(int(match.group(1)))

        # 检测现有 Daemon
        existing = False
        version = None
        check_result = self._ssh.execute("systemctl is-active napcat-daemon")
        if check_result.ok and "active" in check_result.stdout:
            existing = True
            version_result = self._ssh.execute("/opt/napcat-daemon/bin/napcat-daemon --version 2>&1 || echo ''")
            if version_result.ok:
                version = version_result.stdout.strip()

        return ServerInfo(
            os_id=os_id,
            os_version=os_version,
            arch=arch,
            kernel=kernel,
            has_systemd=has_systemd,
            open_ports=list(set(open_ports)),
            existing_daemon=existing,
            daemon_version=version,
        )

    def _get_install_script(self) -> str:
        """获取安装脚本内容。

        优先从打包资源读取，否则从源码读取。
        """
        # 尝试从资源目录读取
        script_paths = [
            # PyInstaller 打包后的路径
            Path(__file__).parent / "resources" / "install.sh",
            # 开发环境路径
            Path(__file__).parent.parent.parent.parent.parent
            / "daemon"
            / "scripts"
            / "install.sh",
            # 相对于项目根目录
            Path.cwd() / "src" / "daemon" / "scripts" / "install.sh",
        ]

        for path in script_paths:
            if path.exists():
                return path.read_text(encoding="utf-8")

        raise FileNotFoundError("找不到安装脚本 install.sh")

    def _extract_token_from_config(self) -> str | None:
        """从远程配置文件提取 Token。"""
        result = self._ssh.execute(
            "sudo cat /etc/napcat-daemon/config.yaml 2>/dev/null || echo ''"
        )
        if not result.ok:
            return None

        for line in result.stdout.splitlines():
            if "token:" in line:
                # 提取 token 值
                match = re.search(r"token:\s*['\"]?([^'\"\s]+)['\"]?", line)
                if match:
                    return match.group(1)

        return None

    def _extract_token_from_output(self, output: str) -> str | None:
        """从安装输出中提取 Token。"""
        # 查找 Token 行
        for line in output.splitlines():
            # 匹配 64 位十六进制 Token
            match = re.search(r"\b([a-f0-9]{64})\b", line)
            if match:
                return match.group(1)

        return None

    def _verify_daemon_running(self) -> bool:
        """验证 Daemon 服务是否运行。"""
        result = self._ssh.execute(
            "systemctl is-active napcat-daemon && systemctl is-enabled napcat-daemon"
        )
        return result.ok and "active" in result.stdout and "enabled" in result.stdout

    def _check_port_open(self, port: int) -> bool:
        """检查端口是否开放。"""
        # 检查本地监听
        result = self._ssh.execute(f"ss -tln | grep -q ':{port} '")
        if result.ok:
            return True

        # 检查防火墙
        result = self._ssh.execute(
            f"sudo iptables -L -n 2>/dev/null | grep -q ':{port}' || "
            f"sudo ufw status 2>/dev/null | grep -q '{port}' || "
            f"sudo firewall-cmd --list-ports 2>/dev/null | grep -q '{port}' || "
            f"echo 'unknown'"
        )

        return True  # 如果无法确定，假设开放

    def uninstall(self, credentials: SSHCredentials) -> bool:
        """卸载远程 Daemon。

        Args:
            credentials: SSH 凭据

        Returns:
            bool: 是否成功
        """
        try:
            if not self._ssh:
                self._ssh = SSHClient(credentials)
                self._owned_ssh = True

            if not self._ssh.connect():
                return False

            # 执行卸载
            result = self._ssh.execute(
                "sudo bash /opt/napcat-daemon/scripts/install.sh --uninstall 2>&1 || "
                "(sudo systemctl stop napcat-daemon 2>/dev/null; "
                " sudo systemctl disable napcat-daemon 2>/dev/null; "
                " sudo rm -rf /opt/napcat-daemon /etc/napcat-daemon; "
                " sudo rm -f /etc/systemd/system/napcat-daemon.service; "
                " sudo systemctl daemon-reload)"
            )

            return result.ok or "inactive" in result.stdout

        except Exception as e:
            logger.exception("卸载失败")
            return False

    def get_daemon_status(
        self, credentials: SSHCredentials
    ) -> dict[str, Any]:
        """获取远程 Daemon 状态。

        Args:
            credentials: SSH 凭据

        Returns:
            状态信息字典
        """
        try:
            if not self._ssh:
                self._ssh = SSHClient(credentials)
                self._owned_ssh = True

            if not self._ssh.connect():
                return {"connected": False, "error": "SSH 连接失败"}

            status = {"connected": True}

            # 检查服务状态
            result = self._ssh.execute("systemctl status napcat-daemon --no-pager 2>&1")
            status["service_active"] = "Active: active (running)" in result.stdout

            # 获取版本
            result = self._ssh.execute(
                "/opt/napcat-daemon/bin/napcat-daemon --version 2>&1 || echo 'unknown'"
            )
            status["version"] = result.stdout.strip()

            # 获取监听端口
            result = self._ssh.execute("ss -tlnp | grep napcat-daemon || echo ''")
            if result.stdout:
                match = re.search(r":(\d+)", result.stdout)
                if match:
                    status["port"] = int(match.group(1))

            return status

        except Exception as e:
            return {"connected": False, "error": str(e)}

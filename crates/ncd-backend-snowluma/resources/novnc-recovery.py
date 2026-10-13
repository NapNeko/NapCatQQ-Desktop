# Repair missing VNC services without restarting the QQ desktop session.
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time


def option(args, name, default=None):
    try:
        return args[args.index(name) + 1]
    except (ValueError, IndexError):
        return default


def endpoint(value):
    host, separator, port = value.rpartition(':')
    if not separator:
        host, port = '', value
    number = int(port)
    if not 1 <= number <= 65535:
        raise ValueError('无效的 VNC 端口')
    return host.strip('[]'), number


def processes():
    result = []
    for proc in Path('/proc').glob('[0-9]*'):
        try:
            if proc.stat().st_uid != os.getuid():
                continue
            args = proc.joinpath('cmdline').read_bytes().decode().strip('\0').split('\0')
            exe = str(proc.joinpath('exe').resolve(strict=True))
            result.append((proc, exe, args))
        except (OSError, UnicodeError):
            continue
    return result


def select_services(items, qq_exe):
    displays = set()
    for proc, exe, args in items:
        if exe == qq_exe and not any(arg.startswith('--type=') for arg in args):
            env = dict(part.split(b'=', 1) for part in proc.joinpath('environ').read_bytes().split(b'\0') if b'=' in part)
            displays.add(env.get(b'DISPLAY', b'').decode())
    if len(displays) != 1 or not next(iter(displays)).startswith(':'):
        raise RuntimeError('无法确定 QQ 图形会话，请确认 Bot 已启动')
    display = displays.pop()
    vnc = [(proc, args) for proc, exe, args in items
           if Path(exe).name == 'x11vnc' and option(args, '-display') == display]
    if len(vnc) > 1:
        raise RuntimeError('QQ 图形会话存在多个 VNC 服务，请检查远端日志')
    port = int(option(vnc[0][1], '-rfbport', '5900')) if vnc else None
    proxies = []
    for proc, exe, args in items:
        # Only the executable/script prefix identifies websockify, never arbitrary -c text.
        if not any(Path(arg).name == 'websockify' for arg in args[:2]) or len(args) < 3:
            continue
        try:
            target_host, target_port = endpoint(args[-1])
            _, listen_port = endpoint(args[-2])
        except ValueError:
            continue
        if target_host in {'localhost', '127.0.0.1', '::1'} and (port is None or port == target_port):
            proxies.append((proc, listen_port, target_port))
    if len(proxies) > 1:
        raise RuntimeError('存在多个 noVNC 代理，无法确定 QQ 连接')
    proxy = proxies[0] if proxies else None
    return display, bool(vnc), port or (proxy[2] if proxy else 5900), proxy


def require_free_port(port):
    with socket.socket() as probe:
        try:
            probe.bind(('127.0.0.1', port))
        except OSError:
            raise RuntimeError('端口 %d 已被占用，未覆盖现有服务' % port) from None


def launch(args, role, runtime, logs):
    with logs.joinpath(role + '.log').open('ab') as log:
        child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                 start_new_session=True, close_fds=True)
    runtime.joinpath('pid_' + role).write_text(str(child.pid))
    return child


def ready(vnc_port, novnc_port):
    try:
        with socket.create_connection(('127.0.0.1', vnc_port), 0.5) as stream:
            if not stream.recv(12).startswith(b'RFB 003.'):
                return False
        connection = http.client.HTTPConnection('127.0.0.1', novnc_port, timeout=0.5)
        try:
            connection.request('GET', '/vnc.html')
            response = connection.getresponse()
            return response.status == 200 and b'<html' in response.read(4096).lower()
        finally:
            connection.close()
    except (OSError, http.client.HTTPException):
        return False


def recover(workspace, runtime, logs, secret, qq_exe):
    display, vnc_running, vnc_port, proxy = select_services(processes(), str(qq_exe.resolve()))
    children = []
    if not vnc_running:
        require_free_port(vnc_port)
        if not secret.is_file() or not secret.stat().st_size:
            raise RuntimeError('VNC 密码文件缺失，无法恢复 QQ 界面')
        children.append(launch(['x11vnc', '-display', display, '-rfbport', str(vnc_port),
                                '-passwdfile', str(secret), '-localhost', '-forever', '-shared',
                                '-noxdamage', '-noxfixes'], 'x11vnc', runtime, logs))
    novnc_port = proxy[1] if proxy else 6081
    if not proxy:
        require_free_port(novnc_port)
        web = next((path for path in [workspace / 'novnc', Path('/usr/share/novnc'), Path('/usr/share/noVNC')]
                    if path.joinpath('vnc.html').is_file()), None)
        if web is None:
            raise RuntimeError('noVNC 页面文件缺失，请修复远端 SnowLuma 组件')
        children.append(launch(['websockify', '--web', str(web), '127.0.0.1:%d' % novnc_port,
                                '127.0.0.1:%d' % vnc_port], 'websockify', runtime, logs))
    deadline = time.monotonic() + 6
    while time.monotonic() < deadline:
        if any(child.poll() is not None for child in children):
            break
        if ready(vnc_port, novnc_port):
            return novnc_port
        time.sleep(0.2)
    raise RuntimeError('VNC 服务未就绪，请检查 x11vnc.log 和 websockify.log')


def main():
    import fcntl
    import signal
    signal.alarm(12)
    os.umask(0o077)
    workspace, runtime, logs, secret, qq_exe = map(Path, sys.argv[1:])
    runtime.mkdir(parents=True, exist_ok=True)
    logs.mkdir(parents=True, exist_ok=True)
    with runtime.joinpath('novnc-recovery.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            port = recover(workspace, runtime, logs, secret, qq_exe)
            print(json.dumps({'port': port}))
        except (RuntimeError, ValueError, OSError) as error:
            print(str(error), file=sys.stderr)
            sys.exit(1)


if __name__ == '__main__':
    main()

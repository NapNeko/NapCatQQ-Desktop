# Repair missing VNC services without restarting the QQ desktop session.
import http.client
import json
import os
from pathlib import Path
import re
import secrets
import socket
import stat
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
            try:
                exe = str(proc.joinpath('exe').resolve(strict=True))
            except PermissionError:
                # Hardened /proc can hide exe even from its owner; QQ itself is validated separately.
                comm = proc.joinpath('comm').read_bytes().decode().strip()
                if comm == 'x11vnc' and args and Path(args[0]).name == 'x11vnc':
                    exe = args[0]
                elif any(Path(arg).name == 'websockify' for arg in args[:2]):
                    exe = ''
                else:
                    continue
            result.append((proc, exe, args))
        except (OSError, UnicodeError):
            continue
    return result


def process_birth(proc):
    fields = proc.joinpath('stat').read_bytes().rsplit(b')', 1)[-1].split()
    if len(fields) < 20 or not fields[19].isdigit():
        raise RuntimeError('无法校验目标 QQ 进程身份，请刷新运行状态')
    return fields[19].decode('ascii')


def target_display(qq_exe, target, proc_root=Path('/proc')):
    pid, uid, start_time = target.get('pid'), target.get('uid'), target.get('start_time')
    display, exe = target.get('display'), target.get('exe')
    if (not isinstance(pid, int) or pid <= 0 or not isinstance(uid, int)
            or not isinstance(start_time, str) or not start_time.isascii() or not start_time.isdigit()
            or not isinstance(exe, str) or not isinstance(display, str)):
        raise RuntimeError('目标 QQ 进程身份无效，请刷新运行状态')
    if uid != os.getuid():
        raise RuntimeError('目标 QQ 进程属于其他用户，未接管其图形会话')
    if Path(exe) != qq_exe.resolve():
        raise RuntimeError('QQ 可执行路径已变化，请刷新组件与运行状态后重试')
    if len(display) > 40 or re.fullmatch(r':[0-9]+(?:\.[0-9]+)?', display) is None:
        raise RuntimeError('目标 QQ 未使用可访问的本地图形会话，请检查 DISPLAY')
    xauthority = target.get('xauthority')
    if xauthority is not None and (not isinstance(xauthority, str) or not xauthority.startswith('/')
                                   or len(xauthority) > 4096):
        raise RuntimeError('目标 QQ 的 XAUTHORITY 路径无效')
    proc = proc_root.joinpath(str(pid))
    try:
        if proc.stat().st_uid != uid:
            raise RuntimeError('目标 QQ 进程属于其他用户，未接管其图形会话')
        if process_birth(proc) != start_time:
            raise RuntimeError('目标 QQ 进程已变化，请刷新 Bot 运行状态')
        return display
    except PermissionError:
        raise RuntimeError('当前 SSH 用户无权检查目标 QQ 进程身份') from None
    except (FileNotFoundError, ProcessLookupError):
        raise RuntimeError('目标 QQ 进程已退出，请确认 Bot 已启动并刷新运行状态') from None


def select_services(items, display):
    vnc = [(proc, args) for proc, exe, args in items
           if Path(exe).name == 'x11vnc' and option(args, '-display') == display]
    if len(vnc) > 1:
        raise RuntimeError('QQ 图形会话存在多个 VNC 服务，请检查远端日志')
    port = int(option(vnc[0][1], '-rfbport', '5900')) if vnc else None
    other_vnc_ports = {int(option(args, '-rfbport', '5900')) for _, exe, args in items
                       if Path(exe).name == 'x11vnc' and option(args, '-display') != display}
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
        if target_host in {'localhost', '127.0.0.1', '::1'} and (port == target_port or port is None and target_port not in other_vnc_ports):
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


def validate_vnc_secret(secret):
    info = secret.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid():
        raise RuntimeError('VNC 密码文件不是当前 SSH 用户拥有的普通文件，未覆盖现有凭据')
    if not info.st_size or info.st_size > 4096:
        raise RuntimeError('VNC 密码文件为空或内容异常，未覆盖现有凭据')
    mode = stat.S_IMODE(info.st_mode)
    if mode & 0o077 or not mode & 0o400:
        raise RuntimeError('VNC 密码文件权限异常，请确认仅当前用户可读')


def ensure_vnc_secret(secret):
    try:
        validate_vnc_secret(secret)
        return
    except FileNotFoundError:
        pass
    except OSError:
        raise RuntimeError('无法检查 VNC 密码文件，请检查目录权限') from None
    try:
        fd = os.open(secret, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    except FileExistsError:
        validate_vnc_secret(secret)
        return
    except OSError:
        raise RuntimeError('创建 VNC 密码文件失败，请检查当前用户的目录写入权限') from None
    created = os.fstat(fd)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write((secrets.token_urlsafe(32) + '\n').encode('ascii'))
            stream.flush()
            os.fsync(stream.fileno())
    except OSError:
        try:
            current = secret.lstat()
            if (current.st_dev, current.st_ino) == (created.st_dev, created.st_ino):
                secret.unlink()
        except OSError:
            pass
        raise RuntimeError('保存 VNC 密码文件失败，未启动 VNC 服务') from None
    validate_vnc_secret(secret)


def launch(args, role, runtime, logs, environment=None):
    with logs.joinpath(role + '.log').open('ab') as log:
        child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                 start_new_session=True, close_fds=True, env=environment)
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


def recover(workspace, runtime, logs, secret, qq_exe, target):
    display = target_display(qq_exe, target)
    display, vnc_running, vnc_port, proxy = select_services(processes(), display)
    children = []
    if not vnc_running:
        require_free_port(vnc_port)
        ensure_vnc_secret(secret)
        environment = os.environ.copy()
        environment['DISPLAY'] = display
        environment.pop('XAUTHORITY', None)
        if target.get('xauthority') is not None:
            environment['XAUTHORITY'] = target['xauthority']
        children.append(launch(['x11vnc', '-display', display, '-rfbport', str(vnc_port),
                                '-passwdfile', str(secret), '-localhost', '-forever', '-shared',
                                '-noxdamage', '-noxfixes'], 'x11vnc', runtime, logs, environment))
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
    workspace, runtime, logs, secret, qq_exe = map(Path, sys.argv[1:6])
    target = json.loads(sys.argv[6])
    runtime.mkdir(parents=True, exist_ok=True)
    logs.mkdir(parents=True, exist_ok=True)
    with runtime.joinpath('novnc-recovery.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            port = recover(workspace, runtime, logs, secret, qq_exe, target)
            print(json.dumps({'port': port}))
        except (RuntimeError, ValueError, OSError) as error:
            print(str(error), file=sys.stderr)
            sys.exit(1)


if __name__ == '__main__':
    main()

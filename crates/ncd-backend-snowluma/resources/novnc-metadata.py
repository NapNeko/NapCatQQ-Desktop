# Read only one attributed QQ process; this helper never starts desktop services.
import json
from pathlib import Path
import re
import sys


class ProbeError(RuntimeError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def process_birth(proc):
    fields = proc.joinpath('stat').read_bytes().rsplit(b')', 1)[-1].split()
    if len(fields) < 20 or not fields[19].isdigit():
        raise ProbeError('invalid_identity', '无法校验目标 QQ 进程身份，请刷新运行状态')
    return fields[19].decode('ascii')


def inspect_target(qq_exe, pid, start_time, qq_id, expected_uid, proc_root=Path('/proc')):
    if (pid <= 0 or qq_id <= 0 or expected_uid < 0
            or not start_time.isascii() or not start_time.isdigit()):
        raise ProbeError('invalid_identity', '目标 QQ 进程身份无效，请刷新运行状态')
    proc = proc_root.joinpath(str(pid))
    try:
        if proc.stat().st_uid != expected_uid:
            raise ProbeError('wrong_user', '目标 QQ 进程属于其他用户，未接管其图形会话')
        if process_birth(proc) != start_time:
            raise ProbeError('identity_changed', '目标 QQ 进程已变化，请刷新 Bot 运行状态')
        try:
            exe = proc.joinpath('exe').resolve(strict=True)
        except FileNotFoundError:
            raise ProbeError('executable_changed', '目标 QQ 可执行文件已变化或已删除，请刷新组件与运行状态') from None
        if exe != qq_exe.resolve():
            raise ProbeError('executable_changed', 'QQ 可执行路径已变化，请刷新组件与运行状态后重试')
        args = proc.joinpath('cmdline').read_bytes().decode('utf-8', 'surrogateescape').strip('\0').split('\0')
        if not args or not args[0] or any(arg.startswith('--type=') for arg in args):
            raise ProbeError('not_main_process', '目标进程不是 QQ 主进程，请刷新 Bot 运行状态')
        accounts = set()
        for index, arg in enumerate(args):
            if arg in {'-q', '--qq', '--qq-id'}:
                value = args[index + 1] if index + 1 < len(args) else ''
            elif arg.startswith(('--qq=', '--qq-id=')):
                value = arg.split('=', 1)[1]
            elif arg.startswith('-q') and len(arg) > 2:
                value = arg[2:]
            else:
                continue
            if not value.isascii() or not value.isdigit():
                raise ProbeError('account_mismatch', '目标 QQ 进程账号参数无效，请刷新 Bot 运行状态')
            accounts.add(int(value))
        if accounts and accounts != {qq_id}:
            raise ProbeError('account_mismatch', '目标 QQ 进程账号与此 Bot 不一致，未打开其他账号的界面')
        try:
            environment = proc.joinpath('environ').read_bytes()
        except PermissionError:
            raise ProbeError('permission_denied', '当前 SSH 用户无权读取目标 QQ 图形会话环境') from None
        values = {}
        for name in [b'DISPLAY', b'XAUTHORITY']:
            prefix = name + b'='
            values[name] = {part[len(prefix):].decode('utf-8', 'surrogateescape')
                            for part in environment.split(b'\0') if part.startswith(prefix)}
            if len(values[name]) > 1:
                raise ProbeError('ambiguous_session', '目标 QQ 环境包含多个 %s，无法确定图形会话' % name.decode())
        display = next(iter(values[b'DISPLAY']), '')
        if len(display) > 40 or re.fullmatch(r':[0-9]+(?:\.[0-9]+)?', display) is None:
            raise ProbeError('missing_display', '目标 QQ 未使用可访问的本地图形会话，请检查 DISPLAY')
        xauthority = next(iter(values[b'XAUTHORITY']), None) or None
        if xauthority is not None and (not xauthority.startswith('/') or len(xauthority) > 4096):
            raise ProbeError('invalid_session', '目标 QQ 的 XAUTHORITY 路径无效')
        if (process_birth(proc) != start_time or proc.stat().st_uid != expected_uid
                or proc.joinpath('exe').resolve(strict=True) != exe):
            raise ProbeError('identity_changed', '读取图形会话期间 QQ 进程已变化，请刷新运行状态')
        return {'pid': pid, 'uid': expected_uid, 'start_time': start_time,
                'exe': str(exe), 'display': display, 'xauthority': xauthority}
    except PermissionError:
        raise ProbeError('permission_denied', '当前 SSH 用户无权检查目标 QQ 进程身份') from None
    except (FileNotFoundError, ProcessLookupError):
        raise ProbeError('not_running', '目标 QQ 进程已退出，请确认 Bot 已启动并刷新运行状态') from None


def main():
    try:
        target = inspect_target(Path(sys.argv[1]), int(sys.argv[2]), sys.argv[3],
                                int(sys.argv[4]), int(sys.argv[5]))
        print(json.dumps({'target': target}))
    except ProbeError as error:
        print(json.dumps({'error': {'code': error.code, 'message': str(error)}}))
        sys.exit(1)
    except (ValueError, IndexError, OSError):
        print(json.dumps({'error': {'code': 'invalid_reply', 'message': '无法读取目标 QQ 图形会话元数据'}}))
        sys.exit(1)


if __name__ == '__main__':
    main()

# Map two bound process identities into the Node PID namespace without enumeration.
import json
import os
from pathlib import Path
import posixpath
import re
import sys


class ProbeError(Exception):
    def __init__(self, code, message):
        self.code = code
        super().__init__(message)


def fail(code, message):
    raise ProbeError(code, message)


def positive(value):
    return type(value) is int and 0 < value <= 4294967295


def valid_pids(values, host_pid):
    return (isinstance(values, list) and 0 < len(values) <= 32
            and all(positive(value) for value in values) and values[0] == host_pid)


def validate(request):
    try:
        node, qq = request['node'], request['qq']
        for process in [node, qq]:
            if (not positive(process['pid']) or not isinstance(process['start_time'], str)
                    or not process['start_time'].isdigit()):
                fail('invalid', '进程 PID 或启动时间无效')
        if (node['pid'] == qq['pid'] or type(node['uid']) is not int or node['uid'] < 0
                or not valid_pids(node['ns_pids'], node['pid'])
                or not re.fullmatch(r'pid:\[[0-9]+\]', node['pid_namespace'])):
            fail('invalid', 'Node 命名空间身份无效')
        for path in [node['exe'], node['cwd'], node['entry'], qq['exe'],
                     request['node_exe'], request['install_dir']]:
            if not isinstance(path, str) or not posixpath.isabs(path) or '\0' in path:
                fail('invalid', '进程身份必须使用明确的绝对路径')
    except (KeyError, TypeError, ValueError):
        fail('invalid', '进程命名空间映射参数无效')


def birth(proc):
    fields = (proc / 'stat').read_text().rsplit(')', 1)[1].split()
    if len(fields) < 20 or not fields[19].isdigit() or fields[0] in ('Z', 'X'):
        fail('changed', '目标进程已退出或启动时间无法确认')
    return fields[19]


def identity(expected, uid, full, proc_root):
    proc = proc_root / str(expected['pid'])
    try:
        before = birth(proc)
        if before != expected['start_time']:
            fail('changed', '目标 PID 的进程身份已经变化')
        status = (proc / 'status').read_text().splitlines()
        uid_fields = next(line.split()[1:] for line in status if line.startswith('Uid:'))
        pids = [int(value) for value in next(
            line.split()[1:] for line in status if line.startswith('NSpid:'))]
        if proc.stat().st_uid != uid or uid_fields != [str(uid)] * 4:
            fail('changed', 'Node 与 QQ 必须属于已确认的 SSH 用户')
        if not valid_pids(pids, expected['pid']):
            fail('ambiguous', '进程 PID 层级无法与宿主机身份对应')
        result = {'pid': expected['pid'], 'start_time': before, 'uid': uid, 'ns_pids': pids}
        if full:
            root = (proc / 'root').stat()
            result.update({
                'exe': os.readlink(proc / 'exe'),
                'cwd': os.readlink(proc / 'cwd'),
                'pid_namespace': os.readlink(proc / 'ns' / 'pid'),
                'root': [root.st_dev, root.st_ino],
            })
            if 'entry' in expected:
                result['args'] = (proc / 'cmdline').read_bytes().decode('utf-8', 'surrogateescape').split('\0')
        if birth(proc) != before or proc.stat().st_uid != uid:
            fail('changed', '进程在只读映射期间发生变化')
        return result
    except PermissionError as error:
        raise ProbeError('permission_denied', '进程元数据受限，需要固定 PID 的只读验证') from error
    except (FileNotFoundError, ProcessLookupError) as error:
        raise ProbeError('changed', '目标进程在只读映射期间退出') from error
    except (OSError, ValueError, IndexError, StopIteration) as error:
        raise ProbeError('unknown', '进程身份或命名空间元数据不完整') from error


def snapshot(request, full, proc_root):
    uid = request['node']['uid']
    return {name: identity(request[name], uid, full, proc_root) for name in ['node', 'qq']}


def strip_snapshot(value):
    keys = ['pid', 'start_time', 'uid', 'ns_pids']
    return {name: {key: value[name][key] for key in keys} for name in ['node', 'qq']}


def node_entry(args):
    no_value_flags = {'--experimental-sqlite', '--enable-source-maps', '--no-warnings',
                      '--trace-warnings', '--expose-gc', '--inspect', '--inspect-brk'}
    value_flags = {'--require', '-r', '--import', '--loader', '--experimental-loader'}
    index = 1
    while index < len(args):
        arg = args[index]
        if arg == '--':
            return args[index + 1] if index + 1 < len(args) else None
        if arg in value_flags:
            index += 2
            continue
        if arg in no_value_flags or any(arg.startswith(flag + '=') for flag in value_flags | {'--inspect', '--inspect-brk'}):
            index += 1
            continue
        if not arg or arg.startswith('-'):
            return None
        return arg
    return None


def probe(mode, request, proc_root=Path('/proc')):
    validate(request)
    uid = request['node']['uid']
    if mode not in ('inspect', 'elevated', 'snapshot'):
        fail('invalid', '进程映射检查模式无效')
    if (mode == 'elevated' and os.getuid() != 0) or (mode != 'elevated' and os.getuid() != uid):
        fail('invalid', '进程映射未绑定原 SSH 用户')
    before = snapshot(request, mode != 'snapshot', proc_root)
    after = snapshot(request, mode != 'snapshot', proc_root)
    if before != after:
        fail('changed', '进程在命名空间验证期间变化')
    compact = strip_snapshot(before)
    if mode == 'snapshot':
        return {'state': 'snapshot', 'snapshot': compact}
    node, qq = before['node'], before['qq']
    expected = request['node']
    install = os.path.realpath(request['install_dir'])
    entry = posixpath.join(install, 'index.mjs')
    script_entry = node_entry(node['args'])
    if (node['exe'] != os.path.realpath(request['node_exe']) or node['exe'] != expected['exe']
            or node['cwd'] != expected['cwd'] or expected['entry'] != entry
            or not (script_entry == entry or (node['cwd'] == install
                    and script_entry in ('index.mjs', './index.mjs')))):
        fail('changed', '已确认的 SnowLuma Node 可执行文件或入口发生变化')
    if qq['exe'] != os.path.realpath(request['qq']['exe']) or not posixpath.isabs(qq['cwd']):
        fail('changed', '目标 QQ 的可执行文件身份不符')
    if (node['pid_namespace'] != expected['pid_namespace'] or node['ns_pids'] != expected['ns_pids']
            or node['pid_namespace'] != qq['pid_namespace'] or node['root'] != qq['root']):
        fail('namespace_mismatch', 'Node 与 QQ 不属于同一 PID 命名空间或进程根目录')
    if len(node['ns_pids']) != len(qq['ns_pids']):
        fail('ambiguous', 'Node 与 QQ 的 PID 命名空间层级存在歧义')
    return {'state': 'mapped', 'process_pid': qq['ns_pids'][-1], 'snapshot': compact}


def main():
    try:
        if len(sys.argv) != 3:
            fail('invalid', '进程映射参数无效')
        result = probe(sys.argv[1], json.loads(sys.argv[2]))
        print(json.dumps(result))
    except ProbeError as error:
        if error.code == 'permission_denied':
            print(json.dumps({'state': 'permission_denied'}))
            return 0
        print(json.dumps({'state': 'error', 'code': error.code, 'message': str(error)}))
        return 1
    except Exception:
        print(json.dumps({'state': 'error', 'code': 'unknown', 'message': '只读进程 PID 映射失败'}))
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())

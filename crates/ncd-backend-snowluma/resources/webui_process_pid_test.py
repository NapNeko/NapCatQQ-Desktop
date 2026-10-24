import copy
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('webui_process_pid', Path(__file__).with_name('webui-process-pid.py'))
pidmap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pidmap)


def stat(pid, start):
    fields = ['S'] * 20
    fields[19] = start
    return f'{pid} (process) ' + ' '.join(fields)


def request(isolated=True):
    return {
        'node': {'pid': 429940, 'uid': 1001, 'start_time': '111', 'exe': '/opt/sl/node',
                 'cwd': '/opt/sl', 'entry': '/opt/sl/index.mjs',
                 'ns_pids': [429940, 19] if isolated else [429940], 'pid_namespace': 'pid:[42]'},
        'qq': {'pid': 430336, 'start_time': '222', 'exe': '/opt/QQ/qq'},
        'node_exe': '/opt/sl/node', 'install_dir': '/opt/sl',
    }


def fixture(isolated=True):
    root = mock.MagicMock()
    procs, fields, links = {}, {}, {}
    for pid, start, exe, cwd, ns_pids in [
            (429940, '111', '/opt/sl/node', '/opt/sl', [429940, 19] if isolated else [429940]),
            (430336, '222', '/opt/QQ/qq', '/opt/QQ', [430336, 46] if isolated else [430336])]:
        proc = mock.MagicMock()
        proc.stat.return_value = SimpleNamespace(st_uid=1001)
        files = {name: mock.MagicMock() for name in ['stat', 'status', 'exe', 'cwd', 'ns', 'root', 'cmdline']}
        files['stat'].read_text.return_value = stat(pid, start)
        files['status'].read_text.return_value = (
            'Uid:\t1001\t1001\t1001\t1001\nNSpid:\t' + '\t'.join(map(str, ns_pids)) + '\n')
        files['root'].stat.return_value = SimpleNamespace(st_dev=8, st_ino=2)
        files['cmdline'].read_bytes.return_value = b'node\0index.mjs\0'
        namespace = mock.MagicMock()
        files['ns'].__truediv__.return_value = namespace
        files['namespace'] = namespace
        proc.__truediv__.side_effect = files.__getitem__
        procs[str(pid)] = proc
        fields[pid] = files
        links[files['exe']], links[files['cwd']], links[namespace] = exe, cwd, 'pid:[42]'
    root.__truediv__.side_effect = procs.__getitem__
    return root, procs, fields, links


class WebuiProcessPidTests(unittest.TestCase):
    def setUp(self):
        self.uid = mock.patch.object(pidmap.os, 'getuid', return_value=1001, create=True)
        self.canonical = mock.patch.object(pidmap.os.path, 'realpath', side_effect=lambda path: path)
        self.uid.start()
        self.canonical.start()
        self.addCleanup(self.uid.stop)
        self.addCleanup(self.canonical.stop)

    def inspect(self, root, links, bound=None):
        with mock.patch.object(pidmap.os, 'readlink', side_effect=links.__getitem__):
            return pidmap.probe('inspect', bound or request(), root)

    def test_same_namespace_maps_host_qq_to_node_visible_pid_without_scanning(self):
        root, _, fields, links = fixture()
        result = self.inspect(root, links)
        self.assertEqual(result['process_pid'], 46)
        root.iterdir.assert_not_called()
        fields[430336]['cmdline'].read_bytes.assert_not_called()

    def test_normal_unisolated_processes_keep_the_host_pid(self):
        root, _, _, links = fixture(False)
        self.assertEqual(self.inspect(root, links, request(False))['process_pid'], 430336)

    def test_service_exact_relative_node_and_index_argv_is_accepted(self):
        root, _, fields, links = fixture()
        fields[429940]['cmdline'].read_bytes.return_value = b'./node\0--experimental-sqlite\0./index.mjs\0'
        self.assertEqual(self.inspect(root, links)['process_pid'], 46)

    def test_index_in_another_scripts_arguments_is_not_an_entry(self):
        for argv in [b'node\0another.mjs\0index.mjs\0',
                     b'node\0another.mjs\0./index.mjs\0',
                     b'node\0another.mjs\0/opt/sl/index.mjs\0',
                     b'node\0--eval\0code\0index.mjs\0']:
            root, _, fields, links = fixture()
            fields[429940]['cmdline'].read_bytes.return_value = argv
            with self.subTest(argv=argv), self.assertRaises(pidmap.ProbeError):
                self.inspect(root, links)

    def test_different_namespace_root_or_namespace_depth_is_rejected(self):
        for change in ['namespace', 'root', 'depth']:
            root, _, fields, links = fixture()
            if change == 'namespace':
                links[fields[430336]['namespace']] = 'pid:[99]'
            elif change == 'root':
                fields[430336]['root'].stat.return_value.st_ino = 99
            else:
                fields[430336]['status'].read_text.return_value = 'Uid:\t1001\t1001\t1001\t1001\nNSpid:\t430336\n'
            with self.subTest(change=change), self.assertRaises(pidmap.ProbeError):
                self.inspect(root, links)

    def test_pid_reuse_or_executable_change_does_not_produce_a_mapping(self):
        root, _, fields, links = fixture()
        fields[430336]['stat'].read_text.return_value = stat(430336, '999')
        with self.assertRaises(pidmap.ProbeError):
            self.inspect(root, links)
        root, _, fields, links = fixture()
        links[fields[430336]['exe']] = '/another/qq'
        with self.assertRaises(pidmap.ProbeError):
            self.inspect(root, links)

    def test_an_exec_or_namespace_change_between_reads_is_rejected(self):
        root, _, fields, links = fixture()
        calls = 0
        def changing(path):
            nonlocal calls
            if path == fields[430336]['namespace']:
                calls += 1
                return 'pid:[42]' if calls == 1 else 'pid:[99]'
            return links[path]
        with mock.patch.object(pidmap.os, 'readlink', side_effect=changing), self.assertRaises(pidmap.ProbeError):
            pidmap.probe('inspect', request(), root)

    def test_other_uid_or_unverified_node_entry_is_rejected(self):
        for change in ['uid', 'entry']:
            root, procs, fields, links = fixture()
            if change == 'uid':
                procs['430336'].stat.return_value.st_uid = 2000
            else:
                fields[429940]['cmdline'].read_bytes.return_value = b'node\0another.mjs\0'
            with self.subTest(change=change), self.assertRaises(pidmap.ProbeError):
                self.inspect(root, links)

    def test_permission_fallback_allows_plain_birth_recheck_without_restricted_links(self):
        root, _, fields, links = fixture()
        def denied(path):
            if path in [fields[429940]['namespace'], fields[430336]['exe']]:
                raise PermissionError()
            return links[path]
        with mock.patch.object(pidmap.os, 'readlink', side_effect=denied), self.assertRaises(pidmap.ProbeError) as error:
            pidmap.probe('inspect', request(), root)
        self.assertEqual(error.exception.code, 'permission_denied')
        with mock.patch.object(pidmap.os, 'getuid', return_value=0), \
                mock.patch.object(pidmap.os, 'readlink', side_effect=links.__getitem__):
            elevated = pidmap.probe('elevated', request(), root)
        with mock.patch.object(pidmap.os, 'readlink', side_effect=PermissionError()) as restricted:
            plain = pidmap.probe('snapshot', request(), root)
        restricted.assert_not_called()
        self.assertEqual(plain['snapshot'], elevated['snapshot'])
        root.iterdir.assert_not_called()

    def test_plain_recheck_rejects_reused_pid_and_unknown_user(self):
        root, _, fields, _ = fixture()
        fields[430336]['stat'].read_text.return_value = stat(430336, '333')
        with self.assertRaises(pidmap.ProbeError):
            pidmap.probe('snapshot', request(), root)
        with mock.patch.object(pidmap.os, 'getuid', return_value=2000), self.assertRaises(pidmap.ProbeError):
            pidmap.probe('snapshot', request(), root)

    def test_missing_namespace_host_pid_or_invalid_bound_pid_is_not_guessed(self):
        root, _, _, links = fixture()
        for field, value in [('pid', 0), ('start_time', ''), ('ns_pids', [19])]:
            bound = copy.deepcopy(request())
            bound['node'][field] = value
            with self.subTest(field=field), self.assertRaises(pidmap.ProbeError):
                self.inspect(root, links, bound)


if __name__ == '__main__':
    unittest.main()

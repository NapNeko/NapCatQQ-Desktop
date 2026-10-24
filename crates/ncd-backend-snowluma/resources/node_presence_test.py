import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('node_presence', Path(__file__).with_name('node-presence.py'))
node = importlib.util.module_from_spec(spec)
spec.loader.exec_module(node)


def process_stat(start_time='12345'):
    fields = ['S'] * 20
    fields[19] = start_time
    return '901 (node) ' + ' '.join(fields)


def fixture(items):
    procs = {}
    links = {}
    fields = {}
    for pid, cwd, exe, args in items:
        proc = mock.MagicMock()
        proc.name = str(pid)
        proc.stat.return_value = SimpleNamespace(st_uid=1000)
        files = {name: mock.Mock() for name in ['stat', 'status', 'comm', 'exe', 'cwd', 'cmdline']}
        files['stat'].read_text.return_value = process_stat()
        files['status'].read_text.return_value = 'Name:\tnode\nUid:\t1000\t1000\t1000\t1000\n'
        files['comm'].read_text.return_value = 'node\n'
        files['cmdline'].read_bytes.return_value = ('\0'.join(args) + '\0').encode()
        proc.__truediv__.side_effect = files.__getitem__
        procs[str(pid)] = proc
        fields[pid] = files
        links[files['exe']] = exe
        links[files['cwd']] = cwd
    root = mock.MagicMock()
    root.iterdir.side_effect = lambda: iter(procs.values())
    root.__truediv__.side_effect = procs.__getitem__
    return root, procs, fields, links


class NodePresenceTests(unittest.TestCase):
    def setUp(self):
        self.uid = mock.patch.object(node.os, 'getuid', return_value=1000, create=True)
        self.realpath = mock.patch.object(node.os.path, 'realpath', side_effect=lambda path: path)
        self.uid.start()
        self.realpath.start()
        self.addCleanup(self.uid.stop)
        self.addCleanup(self.realpath.stop)

    def scan(self, root, links):
        with mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__):
            return node.probe('scan', 1000, '/opt/sl/node', '/opt/sl', proc_root=root)

    def single(self, **overrides):
        return fixture([(901, overrides.get('cwd', '/opt/sl'),
                         overrides.get('exe', '/opt/sl/node'),
                         overrides.get('args', ['node', '--experimental-sqlite', 'index.mjs']))])

    def test_relative_entry_is_confirmed_by_exact_cwd_and_trusted_executable(self):
        root, _, _, links = self.single()
        result = self.scan(root, links)
        self.assertEqual(result['state'], 'present')
        self.assertEqual(result['node'], {'pid': 901, 'uid': 1000, 'start_time': '12345',
                                          'exe': '/opt/sl/node'})

    def test_absolute_entry_can_attribute_a_node_outside_installation_cwd(self):
        root, _, _, links = self.single(cwd='/tmp', args=['node', '/opt/sl/index.mjs'])
        self.assertEqual(self.scan(root, links)['state'], 'present')

    def test_permission_denied_is_not_absent_and_binds_readonly_retry(self):
        root, _, fields, links = self.single()
        def protected(path):
            if path == fields[901]['exe']:
                raise PermissionError()
            return links[path]
        with mock.patch.object(node.os, 'readlink', side_effect=protected):
            result = node.probe('scan', 1000, '/opt/sl/node', '/opt/sl', proc_root=root)
        self.assertEqual(result, {'state': 'permission_denied',
                                  'snapshot': [{'pid': 901, 'start_time': '12345'}]})
        root.iterdir.reset_mock()
        with mock.patch.object(node.os, 'getuid', return_value=0), \
                mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__):
            verified = node.probe('inspect', 1000, '/opt/sl/node', '/opt/sl',
                                  result['snapshot'], proc_root=root)
        self.assertEqual(verified['state'], 'present')
        root.iterdir.assert_not_called()

    def test_two_matching_instances_are_a_conflict_even_with_the_same_exe_and_cwd(self):
        root, _, _, links = fixture([(901, '/opt/sl', '/opt/sl/node', ['node', 'index.mjs']),
                                     (902, '/opt/sl', '/opt/sl/node', ['node', 'index.mjs'])])
        with self.assertRaises(node.ProbeError) as error:
            self.scan(root, links)
        self.assertEqual(error.exception.code, 'conflict')

    def test_only_a_complete_empty_scan_confirms_absence(self):
        root, _, _, links = fixture([])
        self.assertEqual(self.scan(root, links), {'state': 'absent', 'snapshot': []})
        root.iterdir.side_effect = PermissionError()
        with self.assertRaises(PermissionError):
            self.scan(root, links)

    def test_unknown_same_user_process_metadata_does_not_become_absent(self):
        root, _, fields, links = self.single()
        fields[901]['stat'].read_text.side_effect = PermissionError()
        with self.assertRaises(node.ProbeError) as error:
            self.scan(root, links)
        self.assertEqual(error.exception.code, 'unknown')

    def test_unrelated_live_pid_or_same_executable_basename_cannot_prove_readiness(self):
        root, _, _, links = self.single(cwd='/another/install', exe='/another/install/node',
                                        args=['node', 'index.mjs'])
        self.assertEqual(self.scan(root, links)['state'], 'absent')

    def test_related_process_with_an_untrusted_executable_is_not_ignored(self):
        root, _, _, links = self.single(exe='/another/node')
        with self.assertRaises(node.ProbeError) as error:
            self.scan(root, links)
        self.assertEqual(error.exception.code, 'untrusted_exe')

    def test_pid_reuse_or_a_change_during_inspection_is_rejected(self):
        for births in [['67890'], ['12345', '67890']]:
            with self.subTest(births=births):
                root, _, fields, links = self.single()
                fields[901]['stat'].read_text.side_effect = [process_stat(value) for value in births]
                with mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__), \
                        self.assertRaises(node.ProbeError) as error:
                    node.probe('inspect', 1000, '/opt/sl/node', '/opt/sl',
                               [{'pid': 901, 'start_time': '12345'}], proc_root=root)
                self.assertEqual(error.exception.code, 'changed')

    def test_exec_change_with_the_same_pid_and_start_time_is_rejected(self):
        root, _, fields, links = self.single()
        reads = iter(['/opt/sl/node', '/different/node'])
        def changed(path):
            return next(reads) if path == fields[901]['exe'] else links[path]
        with mock.patch.object(node.os, 'readlink', side_effect=changed), \
                self.assertRaises(node.ProbeError) as error:
            node.probe('inspect', 1000, '/opt/sl/node', '/opt/sl',
                       [{'pid': 901, 'start_time': '12345'}], proc_root=root)
        self.assertEqual(error.exception.code, 'changed')

    def test_elevated_inspection_cannot_take_over_another_uid(self):
        root, procs, _, links = self.single()
        procs['901'].stat.return_value.st_uid = 2000
        with mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__), \
                self.assertRaises(node.ProbeError) as error:
            node.probe('inspect', 1000, '/opt/sl/node', '/opt/sl',
                       [{'pid': 901, 'start_time': '12345'}], proc_root=root)
        self.assertEqual(error.exception.code, 'changed')

    def test_scan_rejects_new_or_replaced_candidates_instead_of_selecting_the_first(self):
        root, _, _, links = self.single()
        with mock.patch.object(node, 'snapshot', side_effect=[
                [{'pid': 901, 'start_time': '12345'}],
                [{'pid': 901, 'start_time': '12345'}, {'pid': 902, 'start_time': '12345'}]]), \
                self.assertRaises(node.ProbeError) as error:
            self.scan(root, links)
        self.assertEqual(error.exception.code, 'changed')

    def test_elevated_mode_refuses_unbound_or_invalid_pid_lists(self):
        for candidates in [[], [{'pid': 0, 'start_time': '12345'}],
                           [{'pid': 901, 'start_time': None}]]:
            with self.subTest(candidates=candidates), self.assertRaises(node.ProbeError):
                node.probe('inspect', 1000, '/opt/sl/node', '/opt/sl', candidates)


if __name__ == '__main__':
    unittest.main()

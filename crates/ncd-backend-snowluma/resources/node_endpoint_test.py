import asyncio
import io
from pathlib import Path
import unittest
from unittest import mock

from node_presence_test import fixture, node, process_stat


def endpoint_fixture(ports=(13105,), unrelated=(), ipv6=False):
    root, procs, fields, links = fixture([(901, '/opt/sl', '/opt/sl/node', ['node', 'index.mjs'])])
    files = fields[901]
    files['status'].read_text.return_value = 'Uid:\t1000\t1000\t1000\t1000\nNSpid:\t901\t19\n'
    namespace = mock.MagicMock()
    pid_namespace = mock.Mock()
    namespace.__truediv__.return_value = pid_namespace
    files['ns'] = namespace
    links[pid_namespace] = 'pid:[4026532569]'
    descriptors = mock.MagicMock()
    fds = [mock.Mock() for _ in ports]
    descriptors.iterdir.side_effect = lambda: iter(fds)
    files['fd'] = descriptors
    for index, descriptor in enumerate(fds):
        links[descriptor] = f'socket:[{700 + index}]'
    net = mock.MagicMock()
    tables = {'tcp': mock.Mock(), 'tcp6': mock.Mock()}
    net.__truediv__.side_effect = tables.__getitem__
    files['net'] = net
    rows = ['sl local_address rem_address st tx rx tr tm retr uid timeout inode']
    address = '00000000000000000000000001000000' if ipv6 else '0100007F'
    for index, port in enumerate(ports):
        rows.append(f'0: {address}:{port:04X} 00000000:0000 0A 0 0 0 1000 0 {700 + index}')
    for port in unrelated:
        rows.append(f'0: {address}:{port:04X} 00000000:0000 0A 0 0 0 1000 0 9999')
    table = '\n'.join(rows) + '\n'
    tables['tcp'].open.side_effect = lambda *a, **k: io.StringIO('header\n' if ipv6 else table)
    tables['tcp6'].open.side_effect = lambda *a, **k: io.StringIO(table if ipv6 else 'header\n')
    return root, procs, files, links, fds, pid_namespace


class NodeEndpointTests(unittest.TestCase):
    def setUp(self):
        self.uid = mock.patch.object(node.os, 'getuid', return_value=1000, create=True)
        self.realpath = mock.patch.object(node.os.path, 'realpath', side_effect=lambda path: path)
        self.uid.start()
        self.realpath.start()
        self.addCleanup(self.uid.stop)
        self.addCleanup(self.realpath.stop)

    def scan(self, root, links, matches):
        with mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__), \
                mock.patch.object(node, 'check_http_candidates', new=mock.AsyncMock(return_value=matches)):
            return node.probe('endpoint_scan', 1000, '/opt/sl/node', '/opt/sl', proc_root=root)

    def test_stale_log_port_never_overrides_the_live_owned_runtime_port(self):
        root, _, _, links, _, _ = endpoint_fixture((13105,), unrelated=(13106,))
        result = self.scan(root, links, [True])
        self.assertEqual((result['host'], result['port']), ('127.0.0.1', 13105))
        self.assertEqual(result['listeners'], [{'host': '127.0.0.1', 'port': 13105, 'inode': '700'}])
        self.assertEqual(result['node']['ns_pids'], [901, 19])

    def test_busy_config_port_does_not_hide_the_live_owned_log_port(self):
        root, _, _, links, _, _ = endpoint_fixture((13106,), unrelated=(13105,))
        self.assertEqual(self.scan(root, links, [True])['port'], 13106)

    def test_no_owned_listener_is_explicitly_not_listening(self):
        root, _, _, links, _, _ = endpoint_fixture(())
        with self.assertRaises(node.ProbeError) as error:
            self.scan(root, links, [])
        self.assertEqual(error.exception.code, 'not_listening')

    def test_listener_without_snowluma_http_is_not_a_webui_endpoint(self):
        root, _, _, links, _, _ = endpoint_fixture()
        with self.assertRaises(node.ProbeError) as error:
            self.scan(root, links, [False])
        self.assertEqual(error.exception.code, 'unreachable')

    def test_multiple_snowluma_http_ports_are_a_conflict(self):
        root, _, _, links, _, _ = endpoint_fixture((13105, 13106))
        with self.assertRaises(node.ProbeError) as error:
            self.scan(root, links, [True, True])
        self.assertEqual(error.exception.code, 'conflict')

    def test_ipv6_listener_keeps_its_verified_address(self):
        root, _, _, links, _, _ = endpoint_fixture(ipv6=True)
        self.assertEqual(self.scan(root, links, [True])['host'], '::1')

    def test_permission_unknown_uses_only_bound_births_when_elevated(self):
        root, _, _, links, fds, _ = endpoint_fixture()
        def protected(path):
            if path in fds:
                raise PermissionError()
            return links[path]
        with mock.patch.object(node.os, 'readlink', side_effect=protected):
            denied = node.probe('endpoint_scan', 1000, '/opt/sl/node', '/opt/sl', proc_root=root)
        self.assertEqual(denied['state'], 'permission_denied')
        root.iterdir.reset_mock()
        with mock.patch.object(node.os, 'getuid', return_value=0), \
                mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__), \
                mock.patch.object(node, 'check_http_candidates', new=mock.AsyncMock(return_value=[True])):
            result = node.probe('endpoint_inspect', 1000, '/opt/sl/node', '/opt/sl', denied['snapshot'], proc_root=root)
        self.assertEqual(result['port'], 13105)
        root.iterdir.assert_not_called()

    def test_pid_reuse_during_http_verification_is_rejected(self):
        root, _, fields, links, _, _ = endpoint_fixture()
        async def changed(_):
            fields['stat'].read_text.return_value = process_stat('67890')
            return [True]
        with mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__), \
                mock.patch.object(node, 'check_http_candidates', side_effect=changed), \
                self.assertRaises(node.ProbeError) as error:
            node.probe('endpoint_scan', 1000, '/opt/sl/node', '/opt/sl', proc_root=root)
        self.assertEqual(error.exception.code, 'changed')

    def test_pid_namespace_change_during_http_verification_is_rejected(self):
        root, _, _, links, _, namespace = endpoint_fixture()
        async def changed(_):
            links[namespace] = 'pid:[4026532999]'
            return [True]
        with mock.patch.object(node.os, 'readlink', side_effect=links.__getitem__), \
                mock.patch.object(node, 'check_http_candidates', side_effect=changed), \
                self.assertRaises(node.ProbeError) as error:
            node.probe('endpoint_scan', 1000, '/opt/sl/node', '/opt/sl', proc_root=root)
        self.assertEqual(error.exception.code, 'changed')

    def test_same_installation_cwd_with_another_entry_is_not_snowluma(self):
        root, _, fields, links, _, _ = endpoint_fixture()
        fields['cmdline'].read_bytes.return_value = b'node\0another.mjs\0'
        self.assertEqual(self.scan(root, links, [])['state'], 'absent')

    def test_installation_path_in_another_scripts_argument_is_not_an_entry(self):
        root, _, fields, links, _, _ = endpoint_fixture()
        fields['cmdline'].read_bytes.return_value = b'node\0another.mjs\0/opt/sl/index.mjs\0'
        self.assertEqual(self.scan(root, links, [])['state'], 'absent')

    def test_http_fingerprint_rejects_unrelated_services_and_accepts_auth_gates(self):
        self.assertTrue(node.valid_http_status(200, b'{"status":"running"}'))
        self.assertTrue(node.valid_http_status(401, b'{"status":"failed","message":"Unauthorized"}'))
        self.assertFalse(node.valid_http_status(200, b'{"status":"ok"}'))
        self.assertFalse(node.valid_http_status(401, b'{"error":"Unauthorized"}'))

    def test_http_checks_have_at_most_two_in_flight(self):
        active = 0
        peak = 0
        async def check(_):
            nonlocal active, peak
            active += 1
            peak = max(peak, active)
            await asyncio.sleep(0)
            active -= 1
            return True
        with mock.patch.object(node, 'is_snowluma_http', side_effect=check):
            self.assertEqual(asyncio.run(node.check_http_candidates([{}] * 5)), [True] * 5)
        self.assertEqual(peak, 2)

    def test_chunked_http_response_is_checked_without_authentication(self):
        async def check():
            reader = asyncio.StreamReader()
            body = b'{"status":"running"}'
            reader.feed_data(b'HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n'
                             + f'{len(body):x}\r\n'.encode() + body + b'\r\n0\r\n\r\n')
            reader.feed_eof()
            writer = mock.Mock()
            writer.drain = mock.AsyncMock()
            with mock.patch.object(node.asyncio, 'open_connection', new=mock.AsyncMock(return_value=(reader, writer))):
                self.assertTrue(await node.is_snowluma_http({'host': '127.0.0.1', 'port': 13105}))
            request = writer.write.call_args.args[0]
            self.assertTrue(request.startswith(b'GET /api/status HTTP/1.1\r\n'))
            self.assertNotIn(b'Authorization', request)
            writer.close.assert_called_once()
        asyncio.run(check())


if __name__ == '__main__':
    unittest.main()

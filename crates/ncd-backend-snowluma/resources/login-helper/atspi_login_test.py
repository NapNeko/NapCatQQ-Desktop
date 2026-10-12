# Pure contract tests; no QQ, display, bus, or system library is opened.
import importlib.util
from pathlib import Path
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('atspi_login', Path(__file__).with_name('atspi-login.py'))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class ContractTests(unittest.TestCase):
    def request(self):
        return {'v': 1, 'action': 'snapshot', 'session_id': 'a' * 32, 'scanned': False,
                'binding': {'pid': 13, 'start_time': '1234', 'exe': '/opt/QQ/qq'}}

    def test_request_is_bounded_to_known_control_actions(self):
        helper.validate_request(self.request())
        for action in ('login', 'click', 'spawn', 'kill', 'roundtrip'):
            value = self.request(); value['action'] = action
            with self.assertRaises(ValueError): helper.validate_request(value)

    def test_session_and_process_generation_are_required(self):
        for key, value in [('session_id', 'bad'), ('scanned', 'false')]:
            request = self.request(); request[key] = value
            with self.assertRaises(ValueError): helper.validate_request(request)
        request = self.request(); request['binding']['start_time'] = 'stale'
        with self.assertRaises(ValueError): helper.validate_request(request)

    def test_main_and_scan_evidence_prevent_old_qr_delivery(self):
        qr = {'name': '二维码'}
        self.assertEqual(helper.classify([qr], True), 'login_detected')
        self.assertEqual(helper.classify([qr, {'name': '请在手机上确认登录'}], False), 'scanned')
        self.assertEqual(helper.classify([qr, {'name': '当前二维码已过期'}], False), 'expired')

    def test_window_occlusion_uses_intersection_not_title(self):
        self.assertTrue(helper.overlaps([100, 100, 200, 200], [299, 299, 30, 30]))
        self.assertFalse(helper.overlaps([100, 100, 200, 200], [300, 100, 30, 30]))

    def test_press_is_allowed_next_to_context_menu(self):
        self.assertEqual(helper.default_action_index(['press', 'showContextMenu']), 0)
        self.assertEqual(helper.default_action_index(['showContextMenu', 'press']), 1)
        with self.assertRaises(RuntimeError): helper.default_action_index(['showContextMenu'])
        with self.assertRaises(RuntimeError): helper.default_action_index(['press', 'press'])

    def test_tree_invokes_press_and_never_context_menu(self):
        class Library:
            called = []
            def actionable(self, obj): return True
            def action(self, obj): return 19
            def action_count(self, obj, error): return 2
            def action_name(self, obj, index, error): return ['showContextMenu', 'press'][index]
            def text(self, value): return value
            def do(self, obj, index, error): self.called.append(index); return 1
            def unref(self, value): pass
        tree = helper.Tree.__new__(helper.Tree)
        tree.lib = Library(); tree.main_interface = False
        tree.nodes = [{'obj': 17, 'role': 'push button', 'name': '扫码登录'}]
        tree.invoke('扫码登录', lambda: None)
        self.assertEqual(tree.lib.called, [1])

    def test_account_selector_routes_through_other_methods_then_qr(self):
        pages = [
            [{'name': '其他登录方式', 'role': 'link'}, {'name': '登录', 'role': 'push button'}],
            [{'name': '扫码登录', 'role': 'push button'}, {'name': '账号管理', 'role': 'link'}],
            [{'name': '手机QQ扫码登录', 'role': 'label'}, {'name': '二维码', 'role': 'image'}],
        ]
        calls = []
        class Page:
            main_interface = False
            def __init__(self, nodes): self.nodes = nodes
            def invoke(self, name, check): check(); calls.append(name)
            def close(self): pass
        def next_page(): return Page(pages.pop(0))
        with mock.patch.object(helper.time, 'sleep'):
            page = helper.reach_qr_page(next_page, lambda: None, True)
        self.assertTrue(helper.qr_page(page.nodes))
        self.assertEqual(calls, ['其他登录方式', '扫码登录'])

    def test_quick_login_and_account_management_are_not_navigation(self):
        nodes = [{'name': name, 'role': 'push button'} for name in ('登录', '账号管理', '添加账号')]
        self.assertIsNone(helper.next_login_control(nodes, set()))
        self.assertIsNone(helper.next_login_control([{'name': '扫码登录', 'role': 'label'}], set()))
        self.assertIsNone(helper.next_login_control([{'name': '其他登录方式', 'role': 'link'}], {'其他登录方式'}))

    def test_confirmation_and_main_stop_navigation(self):
        class Page:
            def __init__(self, main):
                self.main_interface = main
                self.nodes = [{'name': '其他登录方式', 'role': 'link'}, {'name': '请在手机上确认登录', 'role': 'label'}]
            def invoke(self, *_): raise AssertionError('confirmation must stop actions')
            def close(self): pass
        for main in (False, True):
            page = Page(main)
            self.assertIs(helper.reach_qr_page(lambda: page, lambda: None, True), page)

    def test_ordinary_poll_does_not_refresh_existing_qr(self):
        class Page:
            main_interface = False
            nodes = [{'name': '手机QQ扫码登录', 'role': 'label'}, {'name': '刷新二维码', 'role': 'push button'}]
            def invoke(self, *_): raise AssertionError('poll must not refresh')
            def close(self): pass
        page = Page()
        self.assertIs(helper.reach_qr_page(lambda: page, lambda: None, False), page)

    def test_two_panel_login_uses_qr_image_and_short_refresh_button(self):
        nodes = [{'name': '扫码登录', 'role': 'static'}, {'name': '二维码', 'role': 'image'},
                 {'name': '刷新', 'role': 'push button'}, {'name': '二维码已过期', 'role': 'static'}]
        self.assertTrue(helper.qr_page(nodes))
        calls = []
        class Page:
            main_interface = False
            def __init__(self, values): self.nodes = values
            def invoke(self, name, check): calls.append(name)
            def close(self): pass
        pages = [Page(nodes), Page(nodes[:2])]
        with mock.patch.object(helper.time, 'sleep'):
            result = helper.reach_qr_page(lambda: pages.pop(0), lambda: None, True)
        self.assertTrue(helper.qr_page(result.nodes))
        self.assertEqual(calls, ['刷新'])

    def test_static_navigation_climbs_only_to_small_owned_click_target(self):
        class Library:
            called = []
            def actionable(self, obj): return True
            def action(self, obj): return obj
            def action_count(self, obj, error): return 2
            def action_name(self, obj, index, error):
                return (['clickAncestor', 'showContextMenu'] if obj == 17 else ['click', 'showContextMenu'])[index]
            def text(self, value): return value
            def parent(self, obj, error): return 19
            def pid(self, obj, error): return 13
            def name(self, obj, error): return ''
            def role(self, obj, error): return 'section'
            def bounds(self, obj): return [399, 235, 81, 30]
            def do(self, obj, index, error): self.called.append((obj, index)); return 1
            def unref(self, value): pass
        tree = helper.Tree.__new__(helper.Tree)
        tree.lib = Library(); tree.main_interface = False; tree.pid = 13
        tree.nodes = [{'obj': 17, 'role': 'static', 'name': '账密登录', 'box': [407, 240, 65, 19]}]
        tree.invoke('账密登录', lambda: None)
        self.assertEqual(tree.lib.called, [(19, 0)])
        tree.lib.bounds = lambda _: [300, 140, 680, 440]
        with self.assertRaises(RuntimeError): tree.invoke('账密登录', lambda: None)
        self.assertEqual(tree.lib.called, [(19, 0)])

    def test_login_document_or_submit_button_cannot_be_navigation_ancestor(self):
        small = [399, 235, 81, 30]; label = [407, 240, 65, 19]
        self.assertTrue(helper.navigation_ancestor('section', '', small, '账密登录', label))
        self.assertFalse(helper.navigation_ancestor('push button', '登录', small, '账密登录', label))
        self.assertFalse(helper.navigation_ancestor('document web', '', small, '账密登录', label))

    def test_dead_session_bus_is_reported_before_accessibility(self):
        connection = mock.MagicMock()
        connection.__enter__.return_value = connection
        connection.connect.side_effect = FileNotFoundError()
        with mock.patch.object(helper.socket, 'socket', return_value=connection), mock.patch.object(helper.socket, 'AF_UNIX', 1, create=True):
            with self.assertRaises(helper.DesktopSessionUnavailable):
                helper.verify_session_bus({'bus': 'unix:path=/tmp/missing-bus', 'uid': 22})

    def test_bus_peer_must_belong_to_bound_user(self):
        connection = mock.MagicMock()
        connection.__enter__.return_value = connection
        with mock.patch.object(helper.socket, 'socket', return_value=connection), mock.patch.object(helper.socket, 'AF_UNIX', 1, create=True), mock.patch.object(helper.socket, 'SO_PEERCRED', 17, create=True):
            connection.getsockopt.return_value = helper.struct.pack('3i', 123, 23, 23)
            with self.assertRaises(helper.DesktopSessionUnavailable):
                helper.verify_session_bus({'bus': 'unix:path=/tmp/private-bus', 'uid': 22})
            connection.getsockopt.return_value = helper.struct.pack('3i', 123, 22, 22)
            helper.verify_session_bus({'bus': 'unix:path=/tmp/private-bus', 'uid': 22})


if __name__ == '__main__': unittest.main()

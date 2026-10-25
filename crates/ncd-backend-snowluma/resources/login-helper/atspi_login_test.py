# Pure contract tests; no QQ, display, bus, or system library is opened.
import importlib.util
from pathlib import Path
import unittest
from unittest import mock
from contextlib import nullcontext

spec = importlib.util.spec_from_file_location('atspi_login', Path(__file__).with_name('atspi-login.py'))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class ContractTests(unittest.TestCase):
    def test_missing_qr_image_is_retryable_instead_of_waiting_for_scan(self):
        for payload in (None, ''):
            with self.subTest(payload=payload), mock.patch.object(
                    helper, 'request_snapshot', return_value=('waiting_for_qr', payload)):
                result = helper.capture_reply(self.request())
                self.assertEqual(result['state'], 'starting')
                self.assertEqual(result['diagnostic'], 'QrImageUnavailable')
                self.assertEqual(result['message'], '正在重新获取二维码')
                self.assertIsNone(result['qrcode_payload'])
                self.assertTrue(result['observer_removed'])

    def test_current_qr_image_and_explicit_login_states_are_preserved(self):
        payload = 'data:image/png;base64,current-code'
        for state, image in [('waiting_for_qr', payload), ('scanned', None),
                             ('expired', None), ('login_detected', None)]:
            with self.subTest(state=state), mock.patch.object(
                    helper, 'request_snapshot', return_value=(state, image)):
                result = helper.capture_reply(self.request())
                self.assertEqual(result['state'], state)
                self.assertEqual(result['qrcode_payload'], image)
                self.assertIsNone(result['diagnostic'])
                self.assertIsNone(result['message'])

    def test_window_change_is_retryable_and_never_returns_stale_material(self):
        for error in [RuntimeError('OwnedWindowNotUnique'), RuntimeError('FrameUnavailable'),
                      TimeoutError(), BlockingIOError()]:
            with mock.patch.object(helper, 'request_snapshot', side_effect=error):
                result = helper.capture_reply(self.request())
                self.assertEqual(result['state'], 'starting')
                self.assertIsNone(result['qrcode_payload'])
                self.assertTrue(result['observer_removed'])

    def test_unknown_dialog_requires_user_action_and_binding_failure_is_not_hidden(self):
        for error in [helper.QrDialogObscured(), ValueError('BindingChanged')]:
            with mock.patch.object(helper, 'request_snapshot', side_effect=error):
                result = helper.capture_reply(self.request())
                self.assertEqual(result['state'], 'unavailable')
                self.assertIsNone(result['qrcode_payload'])

    def test_x11_error_handler_is_restored_when_capture_closes(self):
        lib = mock.Mock()
        lib.open.return_value = 1
        lib.set_error_handler.return_value = None
        display = helper.OwnedDisplay(lib, {'pid': 19, 'display': ':0'})
        self.assertEqual(display.error_handler(None, None), 0)
        display.close()
        lib.close.assert_called_once_with(1)
        lib.set_error_handler.assert_called_with(None)

    def test_shared_refresh_cooldown_reads_current_code_without_clicking(self):
        request = self.request(); request['action'] = 'refresh'
        with mock.patch.object(helper, 'verify_binding'), mock.patch.object(helper, 'process_lease', return_value=nullcontext(7)), \
             mock.patch.object(helper.os, 'pread', return_value=b'99', create=True), \
             mock.patch.object(helper.time, 'monotonic', return_value=100), \
             mock.patch.object(helper, 'snapshot_locked', return_value=('waiting_for_qr', None)) as capture:
            helper.request_snapshot(request)
            self.assertEqual(capture.call_args[0][0]['action'], 'snapshot')

    def test_refresh_after_two_seconds_records_successful_capture(self):
        request = self.request(); request['action'] = 'refresh'
        with mock.patch.object(helper, 'verify_binding'), mock.patch.object(helper, 'process_lease', return_value=nullcontext(7)), \
             mock.patch.object(helper.os, 'pread', return_value=b'95', create=True), \
             mock.patch.object(helper.os, 'ftruncate') as truncate, \
             mock.patch.object(helper.os, 'pwrite', create=True) as write, \
             mock.patch.object(helper.time, 'monotonic', return_value=100), \
             mock.patch.object(helper, 'snapshot_locked', return_value=('waiting_for_qr', 'image')) as capture:
            self.assertEqual(helper.request_snapshot(request), ('waiting_for_qr', 'image'))
            truncate.assert_called_once_with(7, 0)
            write.assert_called_once_with(7, b'100', 0)
            self.assertEqual(capture.call_args[0][0]['action'], 'refresh')

    def test_failed_refresh_does_not_consume_cooldown(self):
        request = self.request(); request['action'] = 'refresh'
        with mock.patch.object(helper, 'verify_binding'), mock.patch.object(helper, 'process_lease', return_value=nullcontext(7)), \
             mock.patch.object(helper.os, 'pread', return_value=b'', create=True), \
             mock.patch.object(helper.os, 'pwrite', create=True) as write, \
             mock.patch.object(helper, 'snapshot_locked', return_value=('expired', None)):
            self.assertEqual(helper.request_snapshot(request), ('expired', None))
            write.assert_not_called()

    def notice_nodes(self):
        return [
            {'obj': 1, 'role': 'dialog', 'name': '', 'dialog': 1},
            {'obj': 2, 'role': 'static', 'name': '下线通知', 'dialog': 1},
            {'obj': 3, 'role': 'push button', 'name': '关闭', 'dialog': 1},
            {'obj': 4, 'role': 'push button', 'name': '关闭', 'dialog': None},
        ]

    def test_notice_close_is_scoped_to_dialog_not_window(self):
        self.assertEqual(helper.notice_close_control(self.notice_nodes()), 3)

    def test_unknown_or_ambiguous_modal_cannot_be_dismissed(self):
        nodes = self.notice_nodes()
        nodes[1]['name'] = '服务协议'
        with self.assertRaises(helper.QrDialogObscured): helper.notice_close_control(nodes)
        nodes = self.notice_nodes() + [{'obj': 5, 'role': 'dialog', 'name': '', 'dialog': 5}]
        with self.assertRaises(helper.QrDialogObscured): helper.notice_close_control(nodes)

    def test_notice_is_closed_before_underlying_qr_refresh(self):
        calls = []
        qr = [{'name': '扫码登录', 'role': 'static'}, {'name': '二维码', 'role': 'image'},
              {'name': '刷新', 'role': 'push button'}]
        class Page:
            main_interface = False
            def __init__(self, nodes): self.nodes = nodes
            def dismiss_notice(self, check): calls.append('dismiss')
            def invoke(self, name, check): calls.append(name)
            def close(self): pass
        pages = [Page(self.notice_nodes() + qr), Page(qr), Page(qr)]
        with mock.patch.object(helper.time, 'sleep'):
            helper.reach_qr_page(lambda: pages.pop(0), lambda: None, True)
        self.assertEqual(calls, ['dismiss', '刷新'])

    def test_persistent_notice_never_returns_obscured_qr(self):
        class Page:
            main_interface = False
            nodes = self.notice_nodes()
            def dismiss_notice(self, check): pass
            def close(self): pass
        with mock.patch.object(helper.time, 'sleep'), self.assertRaises(helper.QrDialogObscured):
            helper.reach_qr_page(Page, lambda: None, False)

    def test_busy_process_never_enters_capture(self):
        with mock.patch.object(helper, 'verify_binding'), \
             mock.patch.object(helper, 'process_lease', side_effect=BlockingIOError), \
             mock.patch.object(helper, 'snapshot_locked') as capture:
            with self.assertRaises(BlockingIOError): helper.request_snapshot(self.request())
            capture.assert_not_called()

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

    def test_phone_confirmation_checks_expiry_without_navigation_or_capture(self):
        request = self.request()
        request.update(scanned=True, action='refresh')
        request['binding'].update(display=':0', bus='unix:path=/tmp/test')
        for names, main, expected in [
            (['请在手机上确认登录'], False, 'scanned'),
            (['二维码'], False, 'scanned'),
            (['二维码已过期'], False, 'expired'),
            ([], True, 'login_detected'),
        ]:
            lib = mock.Mock(); lib.init.return_value = 0
            tree = mock.Mock(nodes=[{'name': name} for name in names], main_interface=main)
            with mock.patch.dict(helper.os.environ), \
                    mock.patch.object(helper, 'verify_binding'), \
                    mock.patch.object(helper, 'verify_session_bus'), \
                    mock.patch.object(helper, 'Libraries', return_value=lib), \
                    mock.patch.object(helper, 'Tree', return_value=tree), \
                    mock.patch.object(helper, 'reach_qr_page') as navigate, \
                    mock.patch.object(helper, 'OwnedDisplay') as capture:
                self.assertEqual(helper.snapshot_locked(request), (expected, None))
                navigate.assert_not_called()
                capture.assert_not_called()
                tree.close.assert_called_once()

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

    def test_named_button_and_its_static_label_are_one_control(self):
        class Library:
            called = []
            def actionable(self, obj): return True
            def action(self, obj): return obj
            def action_count(self, obj, error): return 1
            def action_name(self, obj, index, error): return 'press'
            def text(self, value): return value
            def do(self, obj, index, error): self.called.append(obj); return 1
            def unref(self, value): pass
        for role in ('button', 'push button'):
            with self.subTest(role=role):
                tree = helper.Tree.__new__(helper.Tree)
                tree.lib = Library(); tree.lib.called = []; tree.main_interface = False
                tree.nodes = [{'name': '手机QQ扫码登录', 'role': 'label'},
                              {'obj': 17, 'role': role, 'name': '刷新'},
                              {'obj': 18, 'role': 'static', 'name': '刷新'}]
                tree.invoke('刷新', lambda: None)
                tree.nodes.append({'obj': 19, 'role': 'button', 'name': '刷新'})
                with self.assertRaisesRegex(RuntimeError, 'ActionNotUnique'):
                    tree.invoke('刷新', lambda: None)
                self.assertEqual(tree.lib.called, [17])

    def test_real_qq_button_roles_refresh_the_expired_code_once(self):
        class Library:
            def __init__(self): self.called = []
            def actionable(self, obj): return True
            def action(self, obj): return obj
            def action_count(self, obj, error): return 1
            def action_name(self, obj, index, error): return 'press'
            def text(self, value): return value
            def do(self, obj, index, error): self.called.append((obj, index)); return 1
            def unref(self, value): pass
        lib = Library()
        def page(nodes):
            value = helper.Tree.__new__(helper.Tree)
            value.lib = lib; value.pid = 19; value.refs = []; value.main_interface = False
            value.nodes = nodes
            return value
        title = {'name': '手机QQ扫码登录', 'role': 'static'}
        image = {'name': '二维码', 'role': 'image'}
        alternate = {'obj': 20, 'name': '账密登录', 'role': 'button'}
        expired = page([title, image, alternate,
                        {'name': '当前二维码已过期', 'role': 'static'},
                        {'obj': 17, 'name': '刷新二维码', 'role': 'button'},
                        {'obj': 18, 'name': '刷新', 'role': 'button'}])
        fresh = page([title, image, alternate])
        pages = [expired, fresh]
        with mock.patch.object(helper.time, 'sleep'):
            result = helper.reach_qr_page(lambda: pages.pop(0), lambda: None, True)
        self.assertIs(result, fresh)
        self.assertEqual(lib.called, [(17, 0)])

    def test_real_button_roles_preserve_navigation_and_notice_scopes(self):
        self.assertEqual(helper.next_login_control(
            [{'name': '账密登录', 'role': 'button'}], set()), '账密登录')
        self.assertIsNone(helper.next_login_control(
            [{'name': '登录', 'role': 'button'}, {'name': '账号管理', 'role': 'button'}], set()))
        small, label = [399, 235, 81, 30], [407, 240, 65, 19]
        self.assertTrue(helper.navigation_ancestor('button', '', small, '账密登录', label))
        self.assertFalse(helper.navigation_ancestor('button', '登录', small, '账密登录', label))
        self.assertFalse(helper.navigation_ancestor('button', '', [300, 140, 680, 440], '账密登录', label))
        nodes = self.notice_nodes()
        nodes[2]['role'] = nodes[3]['role'] = 'button'
        self.assertEqual(helper.notice_close_control(nodes), 3)
        nodes.append({'obj': 5, 'role': 'push button', 'name': '关闭', 'dialog': 1})
        with self.assertRaises(helper.QrDialogObscured):
            helper.notice_close_control(nodes)

    def test_expired_overlay_can_outlive_refresh_click(self):
        expired = [{'name': '手机QQ扫码登录', 'role': 'label'},
                   {'name': '当前二维码已过期', 'role': 'static'},
                   {'name': '刷新', 'role': 'push button'}]
        calls, closed = [], []
        class Page:
            main_interface = False
            def __init__(self, nodes): self.nodes = nodes
            def invoke(self, name, check): calls.append(name)
            def close(self): closed.append(self)
        initial, pending, fresh = Page(expired), Page(expired), Page(expired[:1])
        pages = [initial, pending, fresh]
        with mock.patch.object(helper.time, 'sleep'):
            result = helper.reach_qr_page(lambda: pages.pop(0), lambda: None, True)
        self.assertIs(result, fresh)
        self.assertEqual(calls, ['刷新'])
        self.assertEqual(closed, [initial, pending])

    def test_refresh_wait_stops_on_phone_confirmation(self):
        calls = []
        class Page:
            main_interface = False
            def __init__(self, nodes): self.nodes = nodes
            def invoke(self, name, check): calls.append(name)
            def close(self): pass
        pages = [Page([{'name': '手机QQ扫码登录', 'role': 'label'},
                       {'name': '当前二维码已过期', 'role': 'static'},
                       {'name': '刷新', 'role': 'push button'}]),
                 Page([{'name': '请在手机上确认登录', 'role': 'static'}])]
        with mock.patch.object(helper.time, 'sleep'):
            result = helper.reach_qr_page(lambda: pages.pop(0), lambda: None, True)
        self.assertEqual(helper.classify(result.nodes, False), 'scanned')
        self.assertEqual(calls, ['刷新'])

    def test_stuck_expired_overlay_respects_request_deadline(self):
        class Page:
            main_interface = False
            nodes = [{'name': '手机QQ扫码登录', 'role': 'label'},
                     {'name': '当前二维码已过期', 'role': 'static'},
                     {'name': '刷新', 'role': 'push button'}]
            def invoke(self, name, check): pass
            def close(self): pass
        checks = iter([None, None, TimeoutError('RequestTimeout')])
        def check():
            value = next(checks)
            if value: raise value
        with mock.patch.object(helper.time, 'sleep'), self.assertRaises(TimeoutError):
            helper.reach_qr_page(Page, check, True)

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

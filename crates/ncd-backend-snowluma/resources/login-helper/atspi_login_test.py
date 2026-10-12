# Pure contract tests; no QQ, display, bus, or system library is opened.
import importlib.util
from pathlib import Path
import unittest

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


if __name__ == '__main__': unittest.main()

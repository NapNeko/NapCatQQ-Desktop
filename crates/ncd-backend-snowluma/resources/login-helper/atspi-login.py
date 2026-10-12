# Bounded, external QQ login controls and a single owned X11 QR rectangle.
import base64
import ctypes as C
import ctypes.util
import json
import os
from pathlib import Path
import signal
import stat
import struct
import sys
import threading
import time
import zlib

P, I, U, W = C.c_void_p, C.c_int, C.c_uint, C.c_ulong
ALLOWED_ACTIONS = {'snapshot', 'refresh', 'close'}
ALLOWED_BUTTONS = {'账密登录', '扫码登录', '刷新二维码'}
MAX_PAYLOAD = 128 * 1024


def signature(lib, name, result, arguments):
    fn = getattr(lib, name)
    fn.restype, fn.argtypes = result, arguments
    return fn


def dependencies():
    return all(ctypes.util.find_library(name) for name in ('atspi', 'glib-2.0', 'gobject-2.0', 'X11'))


def bind_process(pid, expected_exe):
    if not isinstance(pid, int) or pid <= 0 or not expected_exe.startswith('/'):
        raise ValueError('InvalidBinding')
    proc = Path('/proc/%d' % pid)
    uid = proc.stat().st_uid
    if uid != os.getuid() or (proc / 'exe').resolve() != Path(expected_exe).resolve():
        raise ValueError('ProcessIdentityChanged')
    start = (proc / 'stat').read_text().rsplit(')', 1)[1].split()[19]
    environ = {}
    for item in (proc / 'environ').read_bytes().split(b'\0'):
        key, separator, value = item.partition(b'=')
        if separator and key in {b'DISPLAY', b'DBUS_SESSION_BUS_ADDRESS', b'XAUTHORITY', b'XDG_RUNTIME_DIR'}:
            environ[key.decode()] = value.decode('utf8', errors='strict')
    display, bus = environ.get('DISPLAY', ''), environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    if (not display.startswith(':') or len(display) > 40 or not display[1:].split('.')[0].isdigit()
            or not bus.startswith('unix:') or len(bus) > 1024):
        raise ValueError('LocalSessionUnavailable')
    return {'pid': pid, 'start_time': start, 'exe': str((proc / 'exe').resolve()), 'uid': uid,
            'display': display, 'bus': bus, 'xauthority': environ.get('XAUTHORITY'),
            'runtime_dir': environ.get('XDG_RUNTIME_DIR')}


def verify_binding(binding):
    actual = bind_process(binding['pid'], binding['exe'])
    if any(actual.get(key) != binding.get(key) for key in actual):
        raise ValueError('ProcessIdentityChanged')


def validate_request(request):
    session = request.get('session_id', '')
    if (request.get('v') != 1 or request.get('action') not in ALLOWED_ACTIONS
            or len(session) != 32 or any(char not in '0123456789abcdef' for char in session)
            or not isinstance(request.get('scanned', False), bool)):
        raise ValueError('InvalidRequest')
    binding = request.get('binding')
    if (not isinstance(binding, dict) or type(binding.get('pid')) is not int or binding['pid'] <= 0
            or not isinstance(binding.get('start_time'), str) or not binding['start_time'].isdigit()
            or not isinstance(binding.get('exe'), str) or not binding['exe'].startswith('/')):
        raise ValueError('InvalidBinding')


def overlaps(first, second):
    return (first[0] < second[0] + second[2] and second[0] < first[0] + first[2]
            and first[1] < second[1] + second[3] and second[1] < first[1] + first[3])


def default_action_index(names):
    for allowed in ('press', 'click', 'doDefault'):
        matches = [index for index, name in enumerate(names) if name == allowed]
        if len(matches) == 1:
            return matches[0]
        if len(matches) > 1:
            raise RuntimeError('ActionNotUnique')
    raise RuntimeError('DefaultActionUnavailable')


def classify(nodes, main_interface):
    if main_interface:
        return 'login_detected'
    names = {node['name'] for node in nodes}
    if any(('手机' in name and '确认' in name) or name in {'已扫码', '已扫描', '扫码成功', '扫描成功'} for name in names):
        return 'scanned'
    if any('二维码' in name and ('过期' in name or '失效' in name) for name in names):
        return 'expired'
    return 'waiting_for_qr'


class Rect(C.Structure):
    _fields_ = [('x', I), ('y', I), ('width', I), ('height', I)]


class WindowAttributes(C.Structure):
    _fields_ = [('x', I), ('y', I), ('width', I), ('height', I), ('border', I), ('depth', I),
                ('visual', P), ('root', W), ('window_class', I), ('bit_gravity', I), ('win_gravity', I),
                ('backing_store', I), ('backing_planes', W), ('backing_pixel', W), ('save_under', I),
                ('colormap', W), ('map_installed', I), ('map_state', I), ('all_events', C.c_long),
                ('your_events', C.c_long), ('dont_propagate', C.c_long), ('override_redirect', I), ('screen', P)]


class XImage(C.Structure):
    _fields_ = [('width', I), ('height', I), ('xoffset', I), ('format', I), ('data', P),
                ('byte_order', I), ('bitmap_unit', I), ('bitmap_order', I), ('bitmap_pad', I),
                ('depth', I), ('bytes_per_line', I), ('bits_per_pixel', I),
                ('red_mask', W), ('green_mask', W), ('blue_mask', W)]


class Libraries:
    def __init__(self):
        if not dependencies():
            raise RuntimeError('DependenciesUnavailable')
        at = C.CDLL(ctypes.util.find_library('atspi'))
        gl = C.CDLL(ctypes.util.find_library('glib-2.0'))
        go = C.CDLL(ctypes.util.find_library('gobject-2.0'))
        xl = C.CDLL(ctypes.util.find_library('X11'))
        self.free = signature(gl, 'g_free', None, [P])
        self.iterate = signature(gl, 'g_main_context_iteration', I, [P, I])
        self.unref = signature(go, 'g_object_unref', None, [P])
        self.init = signature(at, 'atspi_init', I, [])
        self.exit = signature(at, 'atspi_exit', I, [])
        self.timeouts = signature(at, 'atspi_set_timeout', None, [I, I])
        self.desktop = signature(at, 'atspi_get_desktop', P, [I])
        self.count = signature(at, 'atspi_accessible_get_child_count', I, [P, P])
        self.child = signature(at, 'atspi_accessible_get_child_at_index', P, [P, I, P])
        self.pid = signature(at, 'atspi_accessible_get_process_id', U, [P, P])
        self.name = signature(at, 'atspi_accessible_get_name', P, [P, P])
        self.role = signature(at, 'atspi_accessible_get_role_name', P, [P, P])
        self.clear = signature(at, 'atspi_accessible_clear_cache', None, [P])
        self.states = signature(at, 'atspi_accessible_get_state_set', P, [P])
        self.has_state = signature(at, 'atspi_state_set_contains', I, [P, I])
        self.action = signature(at, 'atspi_accessible_get_action_iface', P, [P])
        self.action_count = signature(at, 'atspi_action_get_n_actions', I, [P, P])
        self.action_name = signature(at, 'atspi_action_get_action_name', P, [P, I, P])
        self.do = signature(at, 'atspi_action_do_action', I, [P, I, P])
        self.component = signature(at, 'atspi_accessible_get_component_iface', P, [P])
        self.extents = signature(at, 'atspi_component_get_extents', P, [P, I, P])
        self.open = signature(xl, 'XOpenDisplay', P, [C.c_char_p])
        self.close = signature(xl, 'XCloseDisplay', I, [P])
        self.root = signature(xl, 'XDefaultRootWindow', W, [P])
        self.atom = signature(xl, 'XInternAtom', W, [P, C.c_char_p, I])
        self.xfree = signature(xl, 'XFree', I, [P])
        self.query = signature(xl, 'XQueryTree', I, [P, W, C.POINTER(W), C.POINTER(W), C.POINTER(P), C.POINTER(U)])
        self.attrs = signature(xl, 'XGetWindowAttributes', I, [P, W, C.POINTER(WindowAttributes)])
        self.translate = signature(xl, 'XTranslateCoordinates', I, [P, W, W, I, I, C.POINTER(I), C.POINTER(I), C.POINTER(W)])
        self.property = signature(xl, 'XGetWindowProperty', I, [P, W, W, C.c_long, C.c_long, I, W,
                                   C.POINTER(W), C.POINTER(I), C.POINTER(W), C.POINTER(W), C.POINTER(P)])
        self.image = signature(xl, 'XGetImage', P, [P, W, I, I, U, U, W, I])
        self.pixel = signature(xl, 'XGetPixel', W, [P, I, I])
        self.destroy = signature(xl, 'XDestroyImage', I, [P])

    def text(self, pointer):
        if not pointer:
            return ''
        try:
            return C.string_at(pointer).decode('utf8', errors='replace')
        finally:
            self.free(pointer)

    def bounds(self, obj):
        component = self.component(obj)
        if not component:
            return None
        try:
            ptr = self.extents(component, 0, None)
            if not ptr:
                return None
            try:
                value = C.cast(ptr, C.POINTER(Rect)).contents
                return [value.x, value.y, value.width, value.height]
            finally:
                self.free(ptr)
        finally:
            self.unref(component)

    def actionable(self, obj):
        states = self.states(obj)
        if not states:
            return False
        try:
            # AT-SPI StateType: DEFUNCT=6, ENABLED=8, SHOWING=25, VISIBLE=30.
            return (not self.has_state(states, 6) and self.has_state(states, 8)
                    and self.has_state(states, 25) and self.has_state(states, 30))
        finally:
            self.unref(states)


class Tree:
    def __init__(self, lib, pid, check):
        self.lib, self.refs, self.nodes, self.main_interface = lib, [], [], False
        try:
            for _ in range(50):
                if not lib.iterate(None, 0):
                    break
            root = lib.desktop(0)
            if not root:
                return
            self.refs.append(root)
            for index in range(min(max(lib.count(root, None), 0), 64)):
                check()
                app = lib.child(root, index, None)
                if not app:
                    continue
                self.refs.append(app)
                if lib.pid(app, None) != pid:
                    continue
                lib.clear(app)
                pending, count, main = [(app, 0)], 0, set()
                while pending and count < 240:
                    check()
                    obj, depth = pending.pop(0)
                    count += 1
                    name, role = lib.text(lib.name(obj, None)), lib.text(lib.role(obj, None))
                    if role == 'toggle button':
                        if name == '联系人': main.add('contacts')
                        if name.startswith('消息'): main.add('messages')
                        if len(main) == 2:
                            self.main_interface = True
                            self.nodes.clear()
                            return
                    relevant = (name in ALLOWED_BUTTONS | {'手机QQ扫码登录', '二维码', '已扫码', '已扫描', '扫码成功', '扫描成功'}
                                or ('二维码' in name and ('过期' in name or '失效' in name))
                                or ('手机' in name and '确认' in name))
                    if relevant:
                        self.nodes.append({'obj': obj, 'name': name, 'role': role, 'box': lib.bounds(obj)})
                    if depth < 24:
                        for child_index in range(min(max(lib.count(obj, None), 0), 64)):
                            child = lib.child(obj, child_index, None)
                            if child:
                                self.refs.append(child)
                                pending.append((child, depth + 1))
                return
        except BaseException:
            self.close()
            raise

    def close(self):
        for ptr in reversed(self.refs): self.lib.unref(ptr)
        self.refs.clear()

    def invoke(self, name, check):
        if name not in ALLOWED_BUTTONS or self.main_interface:
            raise RuntimeError('ActionRejected')
        matches = [node for node in self.nodes if node['name'] == name and node['role'] == 'push button']
        if len(matches) != 1:
            raise RuntimeError('ActionNotUnique')
        check()
        if not self.lib.actionable(matches[0]['obj']):
            raise RuntimeError('ControlNotActionable')
        action = self.lib.action(matches[0]['obj'])
        if not action:
            raise RuntimeError('ActionMissing')
        try:
            names = [self.lib.text(self.lib.action_name(action, index, None))
                     for index in range(min(max(self.lib.action_count(action, None), 0), 8))]
            index = default_action_index(names)
            check()
            if not self.lib.actionable(matches[0]['obj']):
                raise RuntimeError('ControlNotActionable')
            if not self.lib.do(action, index, None):
                raise RuntimeError('ActionRejected')
        finally:
            self.lib.unref(action)


class OwnedDisplay:
    def __init__(self, lib, binding):
        self.lib, self.pid = lib, binding['pid']
        self.handle = lib.open(binding['display'].encode())
        if not self.handle:
            raise RuntimeError('DisplayUnavailable')
        self.root = lib.root(self.handle)
        self.pid_atom = lib.atom(self.handle, b'_NET_WM_PID', 1)

    def close(self):
        if self.handle: self.lib.close(self.handle)
        self.handle = None

    def children(self, window):
        root, parent, pointer, count = W(), W(), P(), U()
        if not self.lib.query(self.handle, window, C.byref(root), C.byref(parent), C.byref(pointer), C.byref(count)):
            return 0, []
        try:
            if count.value > 256: raise RuntimeError('WindowBudgetExceeded')
            values = list(C.cast(pointer, C.POINTER(W * count.value)).contents) if pointer and count.value else []
            return parent.value, values
        finally:
            if pointer: self.lib.xfree(pointer)

    def owner(self, window):
        actual, fmt, count, after, ptr = W(), I(), W(), W(), P()
        if not self.pid_atom:
            return 0
        self.lib.property(self.handle, window, self.pid_atom, 0, 1, 0, 6,
                          C.byref(actual), C.byref(fmt), C.byref(count), C.byref(after), C.byref(ptr))
        try:
            return C.cast(ptr, C.POINTER(W)).contents.value if ptr and fmt.value == 32 and count.value == 1 else 0
        finally:
            if ptr: self.lib.xfree(ptr)

    def rectangle(self, window):
        attrs, x, y, child = WindowAttributes(), I(), I(), W()
        if (not self.lib.attrs(self.handle, window, C.byref(attrs)) or attrs.map_state != 2
                or not self.lib.translate(self.handle, window, self.root, 0, 0, C.byref(x), C.byref(y), C.byref(child))):
            return None
        return [x.value, y.value, attrs.width, attrs.height]

    def target(self, box, check):
        pending, visited, matches = [self.root], set(), []
        while pending:
            check()
            window = pending.pop()
            if window in visited: continue
            visited.add(window)
            if len(visited) > 256: raise RuntimeError('WindowBudgetExceeded')
            rect = self.rectangle(window)
            if window != self.root and self.owner(window) == self.pid and rect:
                matches.append((window, rect))
            _, children = self.children(window)
            pending.extend(children)
        contained = [(window, rect) for window, rect in matches if
                     box[0] >= rect[0] and box[1] >= rect[1]
                     and box[0] + box[2] <= rect[0] + rect[2] and box[1] + box[3] <= rect[1] + rect[3]]
        if len(contained) != 1: raise RuntimeError('OwnedWindowNotUnique')
        window, rect = contained[0]
        frame = window
        for _ in range(24):
            parent, _ = self.children(frame)
            if parent == self.root: break
            if not parent: raise RuntimeError('WindowParentUnavailable')
            frame = parent
        _, stacking = self.children(self.root)
        if frame not in stacking: raise RuntimeError('WindowStackUnavailable')
        for other in stacking[stacking.index(frame) + 1:]:
            other_rect = self.rectangle(other)
            if other_rect and overlaps(box, other_rect): raise RuntimeError('QrWindowObscured')
        return window, rect

    def png(self, box, check):
        x, y, width, height = box
        if not (64 <= width <= 768 and 64 <= height <= 768): raise RuntimeError('InvalidQrBounds')
        window, rect = self.target(box, check)
        check()
        image = self.lib.image(self.handle, window, x - rect[0], y - rect[1], width, height, W(-1).value, 2)
        if not image: raise RuntimeError('FrameUnavailable')
        raw, dark = bytearray(), 0
        try:
            header = C.cast(image, C.POINTER(XImage)).contents
            masks = [header.red_mask, header.green_mask, header.blue_mask]
            if not all(masks): raise RuntimeError('UnsupportedPixelFormat')
            shifts = [(mask & -mask).bit_length() - 1 for mask in masks]
            peaks = [mask >> shift for mask, shift in zip(masks, shifts)]
            for row in range(height):
                check(); raw.append(0)
                for column in range(width):
                    pixel = self.lib.pixel(image, column, row)
                    rgb = [(((pixel & mask) >> shift) * 255 // peak) for mask, shift, peak in zip(masks, shifts, peaks)]
                    dark += int(max(rgb) < 100); raw.extend(rgb)
            self.target(box, check)
            def chunk(kind, data):
                body = kind + data
                return struct.pack('!I', len(data)) + body + struct.pack('!I', zlib.crc32(body))
            png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!IIBBBBB', width, height, 8, 2, 0, 0, 0))
            png += chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b'')
            payload = 'data:image/png;base64,' + base64.b64encode(png).decode('ascii')
            if len(payload) > MAX_PAYLOAD: raise RuntimeError('QrBudgetExceeded')
            return payload, dark / (width * height)
        finally:
            raw[:] = b'\0' * len(raw)
            self.lib.destroy(image)


def request_snapshot(request):
    binding, initialized, display = request['binding'], False, None
    deadline = time.monotonic() + 5.5
    last_identity_check = [0.0]
    def check():
        if time.monotonic() >= deadline: raise TimeoutError('RequestTimeout')
        cancel = request.get('cancel_path')
        if cancel:
            item = Path(cancel)
            parent = item.parent.stat()
            if (not item.is_absolute() or parent.st_uid != os.getuid() or stat.S_IMODE(parent.st_mode) & 0o077):
                raise ValueError('InvalidCancelPath')
            if item.exists(): raise InterruptedError('RequestCancelled')
        if time.monotonic() - last_identity_check[0] > 0.1:
            verify_binding(binding)
            last_identity_check[0] = time.monotonic()
    check()
    if request['action'] == 'close': return 'cancelled', None
    if request.get('scanned'): return 'scanned', None
    os.environ['DISPLAY'], os.environ['DBUS_SESSION_BUS_ADDRESS'] = binding['display'], binding['bus']
    for key, value in [('XAUTHORITY', binding.get('xauthority')), ('XDG_RUNTIME_DIR', binding.get('runtime_dir'))]:
        if value: os.environ[key] = value
        else: os.environ.pop(key, None)
    for key in ['NO_AT_BRIDGE', 'AT_SPI_BUS_ADDRESS', 'NODE_OPTIONS', 'LD_PRELOAD']:
        os.environ.pop(key, None)
    lib = Libraries()
    try:
        if lib.init() != 0: raise RuntimeError('AtspiUnavailable')
        initialized = True; lib.timeouts(300, 500)
        def tree():
            check(); return Tree(lib, binding['pid'], check)
        current = tree()
        try:
            state = classify(current.nodes, current.main_interface)
            if state in {'login_detected', 'scanned'}: return state, None
            if request['action'] == 'refresh':
                names = {node['name'] for node in current.nodes}
                if '手机QQ扫码登录' not in names:
                    first = '扫码登录' if '扫码登录' in names else '账密登录'
                    current.invoke(first, check); current.close()
                    expected = '扫码登录' if first == '账密登录' else '二维码'
                    while True:
                        current = tree()
                        if classify(current.nodes, current.main_interface) in {'login_detected', 'scanned'}:
                            return classify(current.nodes, current.main_interface), None
                        if any(node['name'] == expected for node in current.nodes): break
                        current.close(); time.sleep(0.1)
                    if first == '账密登录':
                        current.invoke('扫码登录', check); current.close()
                        while True:
                            current = tree()
                            if classify(current.nodes, current.main_interface) in {'login_detected', 'scanned'}:
                                return classify(current.nodes, current.main_interface), None
                            if any(node['name'] == '二维码' for node in current.nodes): break
                            current.close(); time.sleep(0.1)
                elif any(node['name'] == '刷新二维码' for node in current.nodes):
                    current.invoke('刷新二维码', check); current.close(); time.sleep(0.15); current = tree()
            state = classify(current.nodes, current.main_interface)
            if state != 'waiting_for_qr': return state, None
            images = [node for node in current.nodes if node['name'] == '二维码' and node['role'] == 'image' and node['box']]
            if '手机QQ扫码登录' not in {node['name'] for node in current.nodes} or len(images) != 1:
                return 'unavailable', None
            box = images[0]['box']; display = OwnedDisplay(lib, binding)
            payload, ratio = display.png(box, check)
            if not 0.08 <= ratio <= 0.7: return 'waiting_for_qr', None
            after = tree()
            try:
                state = classify(after.nodes, after.main_interface)
                if state != 'waiting_for_qr': return state, None
                boxes = [node['box'] for node in after.nodes if node['name'] == '二维码' and node['role'] == 'image']
                if boxes != [box] or '手机QQ扫码登录' not in {node['name'] for node in after.nodes}:
                    return 'unavailable', None
            finally: after.close()
            check(); return 'waiting_for_qr', payload
        finally: current.close()
    finally:
        if display: display.close()
        if initialized: lib.exit()


def main():
    if '--probe' in sys.argv:
        print(json.dumps({'available': dependencies()})); return
    if '--bind' in sys.argv:
        pid = int(sys.argv[sys.argv.index('--pid') + 1])
        exe = sys.argv[sys.argv.index('--exe') + 1]
        print(json.dumps({'binding': bind_process(pid, exe)})); return
    raw = sys.stdin.buffer.read(8193)
    if len(raw) > 8192: raise ValueError('RequestBudgetExceeded')
    request = json.loads(raw)
    validate_request(request)
    try:
        state, payload = request_snapshot(request)
    except InterruptedError:
        state, payload = 'cancelled', None
    except (RuntimeError, ValueError, OSError, TimeoutError, KeyError):
        state, payload = 'unavailable', None
    print(json.dumps({'session_id': request['session_id'], 'v': 1, 'state': state,
                      'qq_pid': request['binding']['pid'], 'qrcode_payload': payload,
                      'uin': None, 'observer_removed': True,
                      'message': '扫码控件或窗口不可用，请打开 QQ 界面' if state == 'unavailable' else None}, ensure_ascii=False))


if __name__ == '__main__':
    try:
        def watchdog():
            time.sleep(6.5)
            os._exit(124)
        threading.Thread(target=watchdog, daemon=True).start()
        if hasattr(signal, 'SIGALRM'):
            signal.signal(signal.SIGALRM, lambda *_: (_ for _ in ()).throw(TimeoutError('RequestTimeout')))
            signal.alarm(7)
        main()
    except BaseException:
        print(json.dumps({'error': 'LoginHelperUnavailable'})); sys.exit(1)

"""Atomically save a bounded session in an existing private login-assist directory."""
import base64
import json
import os
import pathlib
import stat
import sys
import tempfile


def private_directory(path):
    info = path.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
            or stat.S_IMODE(info.st_mode) != 0o700):
        raise ValueError("invalid session directory")
    return path


def save(request):
    if not isinstance(request, dict) or type(request.get("v")) is not int or request["v"] != 1:
        raise ValueError("invalid request")
    supplied = request.get("directory")
    if not isinstance(supplied, str) or len(supplied) > 4096 or "\0" in supplied:
        raise ValueError("invalid directory")
    directory = pathlib.Path(supplied)
    bot_id = directory.name
    if (not directory.is_absolute() or ".." in directory.parts
            or directory.parent.name != "login-assist"
            or directory.parent.parent.name != "runtime"
            or not bot_id.isascii() or not bot_id.isdecimal() or len(bot_id) > 32):
        raise ValueError("invalid session scope")
    encoded = request.get("body")
    if not isinstance(encoded, str) or len(encoded) > 10924:
        raise ValueError("invalid session body")
    body = base64.b64decode(encoded, validate=True)
    if not body or len(body) > 8192:
        raise ValueError("invalid session size")
    session = json.loads(body)
    if not isinstance(session, dict):
        raise ValueError("invalid session")
    nonce, pid, binding = session.get("session_id"), session.get("qq_pid"), session.get("binding")
    if (session.get("directory") != str(directory) or session.get("expected_uin") != bot_id
            or not isinstance(nonce, str) or len(nonce) != 32
            or any(char not in "0123456789abcdef" for char in nonce.lower())
            or type(pid) is not int or pid <= 0 or not isinstance(binding, dict)
            or type(binding.get("pid")) is not int or binding["pid"] != pid):
        raise ValueError("invalid session identity")
    runtime = directory.parent.parent
    if not stat.S_ISDIR(runtime.lstat().st_mode):
        raise ValueError("invalid runtime directory")
    runtime = runtime.resolve(strict=True)
    assist = private_directory(runtime / "login-assist")
    directory = private_directory(assist / bot_id)
    target = directory / "session.json"
    try:
        info = target.lstat()
    except FileNotFoundError:
        info = None
    if info is not None and (not stat.S_ISREG(info.st_mode)
                             or info.st_uid != os.getuid() or info.st_nlink != 1):
        raise ValueError("invalid session file")
    descriptor, temporary = tempfile.mkstemp(prefix=".session.", dir=directory)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            os.fchmod(stream.fileno(), 0o600)
            stream.write(body)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, target)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
    return {"v": 1, "saved": True}


def main():
    raw = sys.stdin.buffer.read(16385)
    if len(raw) > 16384:
        raise ValueError("request too large")
    print(json.dumps(save(json.loads(raw)), separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("NCD login session save failed", file=sys.stderr)
        raise SystemExit(2)

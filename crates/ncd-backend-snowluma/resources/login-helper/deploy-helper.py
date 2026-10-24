"""Update the private AT-SPI helper without rewriting an unchanged file."""
import base64
import hashlib
import json
import os
import pathlib
import stat
import sys
import tempfile


def private_directory(parent, name):
    path = parent / name
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        pass
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid():
        raise ValueError("invalid helper directory")
    if stat.S_IMODE(metadata.st_mode) != 0o700:
        path.chmod(0o700)
    return path


def deploy(request):
    if not isinstance(request, dict) or request.get("v") != 1:
        raise ValueError("invalid request")
    supplied = request.get("directory")
    if not isinstance(supplied, str) or "\0" in supplied:
        raise ValueError("invalid directory")
    directory = pathlib.Path(supplied)
    bot_id = directory.name
    if (not directory.is_absolute() or ".." in directory.parts
            or directory.parent.name != "login-assist"
            or directory.parent.parent.name != "runtime"
            or not bot_id.isascii() or not bot_id.isdecimal() or len(bot_id) > 32):
        raise ValueError("invalid helper scope")
    encoded = request.get("body")
    if not isinstance(encoded, str) or len(encoded) > 350000:
        raise ValueError("invalid helper body")
    body = base64.b64decode(encoded, validate=True)
    if not body or len(body) > 262144:
        raise ValueError("invalid helper size")
    digest = hashlib.sha256(body).digest()
    runtime = directory.parent.parent.resolve(strict=True)
    assist = private_directory(runtime, "login-assist")
    directory = private_directory(assist, bot_id)
    target = directory / "atspi-login.py"
    try:
        metadata = target.lstat()
    except FileNotFoundError:
        metadata = None
    if metadata is not None and (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()):
        raise ValueError("invalid helper file")
    try:
        descriptor = os.open(target, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
    except FileNotFoundError:
        descriptor = None
    if descriptor is not None:
        with os.fdopen(descriptor, "rb") as stream:
            metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid():
                raise ValueError("invalid helper file")
            current = stream.read(262145)
            if hashlib.sha256(current).digest() == digest:
                if stat.S_IMODE(metadata.st_mode) != 0o600:
                    os.fchmod(stream.fileno(), 0o600)
                return {"v": 1, "changed": False}
    descriptor, temporary = tempfile.mkstemp(prefix=".atspi-login.", dir=directory)
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
    return {"v": 1, "changed": True}


def main():
    raw = sys.stdin.buffer.read(360001)
    if len(raw) > 360000:
        raise ValueError("request too large")
    print(json.dumps(deploy(json.loads(raw)), separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("NCD login helper deployment failed", file=sys.stderr)
        raise SystemExit(2)

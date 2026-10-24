"""Atomically synchronize three fixed NCD metrics assets, without running commands."""
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import tempfile


def directory(parent, name):
    path = parent / name
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        pass
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or path.is_symlink():
        raise ValueError("unsafe_metrics_directory")
    return path


def synchronize(request):
    if not isinstance(request, dict) or request.get("v") != 1:
        raise ValueError("invalid_request")
    home, bot = request.get("home"), request.get("bot_id")
    if (not isinstance(home, str) or not os.path.isabs(home) or "\0" in home
            or not isinstance(bot, str) or not bot.isascii() or not bot.isdecimal() or len(bot) > 32):
        raise ValueError("invalid_scope")
    expected = {"ncd-ob11-stats.cjs", bot + "/nodes.json", "snowluma-shared/" + bot + ".json"}
    assets = request.get("assets")
    if not isinstance(assets, list) or len(assets) != 3:
        raise ValueError("invalid_assets")
    prepared = {}
    for item in assets:
        if not isinstance(item, dict) or item.get("path") not in expected or item["path"] in prepared:
            raise ValueError("invalid_asset_path")
        if not isinstance(item.get("body"), str):
            raise ValueError("invalid_asset_body")
        body = item["body"].encode("utf-8")
        if len(body) > 262144 or hashlib.sha256(body).hexdigest() != item.get("sha256"):
            raise ValueError("invalid_asset_digest")
        prepared[item["path"]] = body
    if set(prepared) != expected:
        raise ValueError("incomplete_assets")
    root = directory(directory(Path(home).resolve(strict=True), "ncd-watch"), "metrics")
    changed = 0
    for relative, body in prepared.items():
        parts = relative.split("/")
        parent = directory(root, parts[0]) if len(parts) == 2 else root
        path = parent / parts[-1]
        previous = None
        try:
            previous = path.lstat()
            if not stat.S_ISREG(previous.st_mode) or path.is_symlink():
                raise ValueError("unsafe_metrics_file")
            descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
            with os.fdopen(descriptor, "rb") as stream:
                if stream.read(262145) == body:
                    continue
        except FileNotFoundError:
            pass
        descriptor, temporary = tempfile.mkstemp(prefix=".ncd-assets-", dir=parent)
        try:
            with os.fdopen(descriptor, "wb") as stream:
                if previous is not None:
                    os.chmod(temporary, stat.S_IMODE(previous.st_mode))
                    if hasattr(os, "fchown"):
                        os.fchown(stream.fileno(), previous.st_uid, previous.st_gid)
                stream.write(body)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, path)
            changed += 1
        finally:
            try:
                os.unlink(temporary)
            except FileNotFoundError:
                pass
    return {"v": 1, "checked": 3, "changed": changed}


def main():
    raw = sys.stdin.buffer.read(1048577)
    if len(raw) > 1048576:
        raise ValueError("request_limit")
    print(json.dumps(synchronize(json.loads(raw)), separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, TypeError, KeyError):
        print("NCD metrics asset synchronization failed", file=sys.stderr)
        sys.exit(2)

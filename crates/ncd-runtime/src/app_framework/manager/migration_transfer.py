"""迁移只读检查、停写快照及校验恢复；不跟随项目中的链接。"""

import hashlib
from contextlib import closing
import io
import json
import os
import re
from pathlib import Path, PurePosixPath
import shutil
import sqlite3
import stat
import sys
import tarfile

MANIFEST = ".ncd-migration-manifest.json"
SKIP = {".venv", "venv", "node_modules", ".ncd-python", "__pycache__",
        ".pytest_cache", ".mypy_cache", ".ruff_cache"}


def digest(path):
    result = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def inventory(root, include_dirs=False):
    if not root.is_absolute() or root.is_symlink() or not root.is_dir():
        raise ValueError("项目目录必须是绝对路径且不能是链接")
    if root.resolve() == Path("/"):
        raise ValueError("不能迁移根目录")
    result = []
    pending = [root]
    while pending:
        parent = pending.pop()
        for item in sorted(parent.iterdir()):
            if item.name in SKIP or item.name.startswith(".ncd-") or item.suffix == ".pyc":
                continue
            info = item.lstat()
            if stat.S_ISLNK(info.st_mode):
                raise ValueError("项目包含链接，需先处理外部数据: " + str(item.relative_to(root)))
            if stat.S_ISDIR(info.st_mode):
                if include_dirs:
                    result.append((item, info))
                pending.append(item)
            elif stat.S_ISREG(info.st_mode):
                result.append((item, info))
            else:
                raise ValueError("项目包含不支持的特殊文件: " + str(item.relative_to(root)))
    return result


def signature(info):
    return info.st_size, info.st_mtime_ns, info.st_ino


def no_writers(root):
    # 额外后台/图片服务可能仍在写同一目录，不能只凭 NCD 主进程退出就打快照。
    resolved = root.resolve()
    for process in Path("/proc").iterdir():
        if not process.name.isdigit() or int(process.name) == os.getpid():
            continue
        try:
            cwd = (process / "cwd").resolve(strict=True)
            if cwd == resolved or resolved in cwd.parents:
                raise ValueError("项目还有进程在运行，未生成快照 (PID " + process.name + ")")
        except (FileNotFoundError, ProcessLookupError):
            continue
        except PermissionError:
            continue


def check_target(root, needed):
    if root.exists():
        raise ValueError("目标迁移目录已存在，拒绝覆盖")
    parent = root.parent
    while not parent.exists():
        parent = parent.parent
    free = shutil.disk_usage(parent).free
    if free < needed * 3 + 512 * 1024 * 1024:
        raise ValueError("目标磁盘空间不足，需要为归档和依赖重建留出空间")
    if not os.access(parent, os.W_OK | os.X_OK):
        raise ValueError("目标目录没有写入权限")
    return {"bytes": needed, "files": 0, "free": free, "sha256": ""}


def pack(root, archive):
    no_writers(root)
    files = inventory(root, include_dirs=True)
    if archive.exists():
        raise ValueError("快照已存在，拒绝覆盖")
    archive.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    rows = {}
    with open(archive, "xb") as output:
        os.chmod(archive, 0o600)
        with tarfile.open(fileobj=output, mode="w:gz", dereference=True) as bundle:
            for path, info in files:
                rel = path.relative_to(root).as_posix()
                directory = stat.S_ISDIR(info.st_mode)
                rows[rel] = {"bytes": 0 if directory else info.st_size,
                             "sha256": "" if directory else digest(path), "directory": directory}
                bundle.add(path, arcname=rel, recursive=False)
                if signature(path.stat()) != signature(info):
                    raise ValueError("打包期间文件发生变化: " + rel)
            payload = json.dumps(rows, ensure_ascii=False).encode("utf-8")
            member = tarfile.TarInfo(MANIFEST)
            member.size = len(payload)
            member.mode = 0o600
            bundle.addfile(member, io.BytesIO(payload))
    for path, info in files:
        if signature(path.stat()) != signature(info):
            raise ValueError("快照结束前文件发生变化: " + str(path.relative_to(root)))
    if {p.relative_to(root).as_posix() for p, _ in inventory(root, include_dirs=True)} != set(rows):
        raise ValueError("打包期间项目文件列表发生变化")
    no_writers(root)
    return {"bytes": archive.stat().st_size, "files": len(rows),
            "sha256": digest(archive), "free": 0}


def safe_name(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or ".." in path.parts or "\\" in name:
        raise ValueError("归档路径不安全")
    if path.as_posix() != name or name.startswith("./"):
        raise ValueError("归档路径不规范")
    return path


def restore(root, archive, expected, old_root, managed_root):
    os.chmod(archive, 0o600)
    if digest(archive) != expected:
        raise ValueError("上传归档 SHA-256 不一致")
    managed = Path(managed_root).resolve()
    destination = root.resolve()
    if destination == managed or managed not in destination.parents:
        raise ValueError("目标目录越过托管根目录")
    if root.exists():
        raise ValueError("目标目录已存在，拒绝覆盖")
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        names = set()
        for member in members:
            safe_name(member.name)
            if not (member.isfile() or member.isdir()) or member.name in names:
                raise ValueError("归档包含链接、目录或重复条目")
            names.add(member.name)
        manifest_info = bundle.getmember(MANIFEST)
        manifest = bundle.extractfile(manifest_info)
        if manifest is None or manifest_info.size > 64 * 1024 * 1024:
            raise ValueError("归档缺少有效校验清单")
        rows = json.load(manifest)
        if names != set(rows) | {MANIFEST}:
            raise ValueError("归档文件列表与清单不一致")
        check_target(root, sum(row["bytes"] for row in rows.values()))
        root.mkdir(mode=0o700, parents=True, exist_ok=False)
        for member in members:
            if member.name == MANIFEST:
                continue
            path = root.joinpath(*safe_name(member.name).parts)
            row = rows[member.name]
            if member.isdir():
                if not row.get("directory"):
                    raise ValueError("目录条目与清单不一致")
                path.mkdir(mode=member.mode & 0o777, parents=True, exist_ok=True)
                continue
            if row.get("directory"):
                raise ValueError("文件条目与清单不一致")
            path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            source = bundle.extractfile(member)
            with open(path, "xb") as output:
                shutil.copyfileobj(source, output, 1024 * 1024)
            os.chmod(path, member.mode & 0o777)
            if path.stat().st_size != row["bytes"] or digest(path) != row["sha256"]:
                raise ValueError("恢复文件校验失败: " + member.name)
    for name in rows:
        if rows[name].get("directory"):
            continue
        path = root / name
        with open(path, "rb") as stream:
            header = stream.read(16)
        if header == b"SQLite format 3\x00":
            with closing(sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)) as database:
                if database.execute("PRAGMA quick_check").fetchall() != [("ok",)]:
                    raise ValueError("SQLite 校验失败: " + name)
        is_config = path.name.startswith(".env") or path.suffix.lower() in {".json", ".toml", ".yaml", ".yml"}
        if is_config and ".git" not in path.parts and path.stat().st_size < 2 * 1024 * 1024:
            try:
                content = path.read_text(encoding="utf-8")
            except UnicodeError:
                continue
            rewritten = re.sub(re.escape(old_root.rstrip("/")) + r"(?=/|[\"'\s]|$)",
                               lambda _: str(root).rstrip("/"), content)
            if rewritten != content:
                path.write_text(rewritten, encoding="utf-8")
    return {"bytes": archive.stat().st_size, "files": len(rows), "sha256": expected, "free": 0}


def main():
    action, raw_root, *args = sys.argv[1:]
    root = Path(raw_root)
    if action == "inspect":
        staging = (Path.home() / "ncd" / "migrations").resolve()
        if root.resolve() == staging or root.resolve() in staging.parents:
            raise ValueError("项目包含 NCD 的迁移暂存目录，请使用具体项目目录")
        files = inventory(root)
        result = {"bytes": sum(info.st_size for _, info in files), "files": len(files), "free": 0, "sha256": ""}
    elif action == "target":
        result = check_target(root, int(args[0]))
    elif action == "pack":
        result = pack(root, Path(args[0]))
    elif action == "stage":
        managed = Path(args[0]).resolve()
        if root.is_symlink() or managed not in root.resolve().parents:
            raise ValueError("快照目录越过托管根目录")
        root.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(root, 0o700)
        result = {"bytes": 0, "files": 0, "free": 0, "sha256": ""}
    elif action == "restore":
        result = restore(root, Path(args[0]), args[1], args[2], args[3])
    else:
        raise ValueError("未知迁移动作")
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)

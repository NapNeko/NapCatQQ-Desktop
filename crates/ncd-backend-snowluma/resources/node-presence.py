# Read only same-user Node identities; elevated inspection receives bound PIDs only.
import json
import asyncio
import ipaddress
import os
from pathlib import Path
import posixpath
import sys
import re


class ProbeError(Exception):
    def __init__(self, code, message):
        self.code = code
        super().__init__(message)


def birth(proc):
    fields = (proc / "stat").read_text().rsplit(")", 1)[1].split()
    if len(fields) < 20 or not fields[19].isdigit():
        raise ProbeError("unknown", "无法读取 Node 进程启动时间，未启动新实例")
    return fields[19], fields[0]


def snapshot(expected_uid, proc_root=Path("/proc")):
    result = []
    for proc in proc_root.iterdir():
        if not proc.name.isdigit():
            continue
        try:
            if proc.stat().st_uid != expected_uid:
                continue
            comm = (proc / "comm").read_text().strip()
            if comm not in ("node", "nodejs"):
                continue
            start_time, state = birth(proc)
            if state not in ("Z", "X"):
                result.append({"pid": int(proc.name), "start_time": start_time})
                if len(result) > 256:
                    raise ProbeError("unknown", "Node 候选进程过多，未启动新实例")
        except FileNotFoundError:
            continue
        except (OSError, ValueError, IndexError) as error:
            raise ProbeError("unknown", "无法确认当前用户的 Node 进程，未启动新实例") from error
    if len(result) > 256:
        raise ProbeError("unknown", "Node 候选进程过多，未启动新实例")
    return sorted(result, key=lambda item: item["pid"])


def validate_candidates(candidates):
    if not isinstance(candidates, list) or len(candidates) > 256:
        raise ProbeError("invalid", "Node 身份检查参数无效")
    seen = set()
    for item in candidates:
        if (not isinstance(item, dict) or set(item) != {"pid", "start_time"}
                or type(item["pid"]) is not int or item["pid"] <= 0
                or not isinstance(item["start_time"], str)
                or not item["start_time"].isdigit() or item["pid"] in seen):
            raise ProbeError("invalid", "Node 身份检查参数无效")
        seen.add(item["pid"])


def node_entry(args):
    no_value_flags = {"--experimental-sqlite", "--enable-source-maps", "--no-warnings",
                      "--trace-warnings", "--expose-gc", "--inspect", "--inspect-brk"}
    value_flags = {"--require", "-r", "--import", "--loader", "--experimental-loader"}
    index = 1
    while index < len(args):
        arg = args[index]
        if arg == "--":
            return args[index + 1] if index + 1 < len(args) else None
        if arg in value_flags:
            index += 2
            continue
        if arg in no_value_flags or any(arg.startswith(flag + "=") for flag in value_flags | {"--inspect", "--inspect-brk"}):
            index += 1
            continue
        if not arg or arg.startswith("-"):
            return None
        return arg
    return None


def inspect_one(item, expected_uid, node_exe, sl_dir, proc_root=Path("/proc"), endpoint=False):
    proc = proc_root / str(item["pid"])
    try:
        before, state = birth(proc)
        if before != item["start_time"] or state in ("Z", "X"):
            raise ProbeError("changed", "Node 进程身份已变化，未启动新实例")
        status_lines = (proc / "status").read_text().splitlines()
        uid_fields = next(line.split()[1:] for line in status_lines
                          if line.startswith("Uid:"))
        if proc.stat().st_uid != expected_uid or uid_fields != [str(expected_uid)] * 4:
            raise ProbeError("changed", "Node 进程用户身份已变化，未启动新实例")
        if (proc / "comm").read_text().strip() not in ("node", "nodejs"):
            raise ProbeError("changed", "目标 PID 已不再是 Node，未启动新实例")
        exe = os.readlink(proc / "exe")
        cwd = os.readlink(proc / "cwd")
        args = (proc / "cmdline").read_bytes().decode("utf-8", "surrogateescape").split("\0")
        if endpoint:
            ns_pids = [int(value) for value in next(
                line.split()[1:] for line in status_lines if line.startswith("NSpid:"))]
            namespace = os.readlink(proc / "ns" / "pid")
            if (not ns_pids or ns_pids[0] != item["pid"] or any(value <= 0 for value in ns_pids)
                    or len(ns_pids) > 32 or not re.fullmatch(r"pid:\[[0-9]+\]", namespace)):
                raise ProbeError("unknown", "Node PID 命名空间元数据无效")
        after, final_state = birth(proc)
        if after != before or final_state in ("Z", "X") or proc.stat().st_uid != expected_uid:
            raise ProbeError("changed", "Node 进程在检查期间发生变化，未启动新实例")
        if (os.readlink(proc / "exe") != exe or os.readlink(proc / "cwd") != cwd
                or (proc / "cmdline").read_bytes().decode("utf-8", "surrogateescape").split("\0") != args):
            raise ProbeError("changed", "Node 可执行文件或入口在检查期间变化，未启动新实例")
    except PermissionError as error:
        raise ProbeError("permission_denied", "Node 进程元数据受限，需要只读身份验证") from error
    except (FileNotFoundError, ProcessLookupError) as error:
        raise ProbeError("changed", "Node 进程在检查期间退出，未启动新实例") from error
    except (OSError, ValueError, IndexError, StopIteration) as error:
        raise ProbeError("unknown", "Node 进程元数据不完整，未启动新实例") from error
    # A relative index.mjs is attributable only with the verified installation cwd.
    entry = posixpath.join(sl_dir, "index.mjs")
    related = cwd == sl_dir or entry in args
    if endpoint and related:
        script_entry = node_entry(args)
        if script_entry is None:
            raise ProbeError("unknown", "无法可靠解析安装目录 Node 的启动入口")
        related = script_entry == entry or (cwd == sl_dir and script_entry in ("index.mjs", "./index.mjs"))
    if not related:
        return None
    if exe != node_exe:
        raise ProbeError("untrusted_exe", "安装目录中的 Node 可执行文件不符，未启动新实例")
    node = {**item, "uid": expected_uid, "exe": exe}
    if endpoint:
        node.update(cwd=cwd, entry=entry, ns_pids=ns_pids, pid_namespace=namespace)
    return node


def inspect_candidates(candidates, expected_uid, node_exe, sl_dir, proc_root=Path("/proc"), endpoint=False):
    validate_candidates(candidates)
    found = []
    denied = False
    for item in candidates:
        try:
            node = inspect_one(item, expected_uid, node_exe, sl_dir, proc_root, endpoint)
            if node is not None:
                found.append(node)
        except ProbeError as error:
            if error.code != "permission_denied":
                raise
            denied = True
    if len(found) > 1:
        raise ProbeError("conflict", "同一 SnowLuma 安装存在多个 Node 实例，未启动新实例")
    if denied:
        return {"state": "permission_denied", "snapshot": candidates}
    if found:
        return {"state": "present", "node": found[0], "snapshot": candidates}
    return {"state": "absent", "snapshot": candidates}


def listening_sockets(node, proc_root=Path("/proc")):
    proc = proc_root / str(node["pid"])
    try:
        inodes = set()
        for index, descriptor in enumerate((proc / "fd").iterdir()):
            if index >= 4096:
                raise ProbeError("unknown", "Node 文件描述符过多，无法完整确认监听端口")
            try:
                target = os.readlink(descriptor)
            except FileNotFoundError:
                continue
            match = re.fullmatch(r"socket:\[([0-9]+)\]", target)
            if match:
                inodes.add(match.group(1))
        listeners = set()
        for family, table in ((4, "tcp"), (6, "tcp6")):
            try:
                with (proc / "net" / table).open("r", encoding="ascii") as stream:
                    contents = stream.read(2 * 1024 * 1024 + 1)
            except FileNotFoundError:
                if family == 6:
                    continue
                raise
            if len(contents) > 2 * 1024 * 1024:
                raise ProbeError("unknown", "Node 网络表过大，无法完整确认监听端口")
            for line in contents.splitlines()[1:]:
                fields = line.split()
                if len(fields) < 10 or fields[3] != "0A" or fields[9] not in inodes:
                    continue
                address, raw_port = fields[1].split(":")
                port = int(raw_port, 16)
                raw = bytes.fromhex(address)
                if family == 4:
                    ip = ipaddress.IPv4Address(raw[::-1])
                else:
                    ip = ipaddress.IPv6Address(b"".join(raw[index:index + 4][::-1] for index in range(0, 16, 4)))
                host = ("127.0.0.1" if family == 4 else "::1") if ip.is_unspecified else str(ip)
                if 0 < port <= 65535:
                    listeners.add((host, port, fields[9]))
        if len(listeners) > 16 or len({port for _, port, _ in listeners}) > 8:
            raise ProbeError("unknown", "Node 监听端口过多，无法唯一确认 WebUI")
        return [{"host": host, "port": port, "inode": inode} for host, port, inode in sorted(listeners)]
    except PermissionError as error:
        raise ProbeError("permission_denied", "Node socket 元数据受限，需要只读验证") from error
    except (FileNotFoundError, ProcessLookupError) as error:
        raise ProbeError("changed", "Node 在端口检查期间退出") from error
    except (OSError, ValueError, IndexError) as error:
        raise ProbeError("unknown", "无法完整读取 Node 的监听 socket") from error


def valid_http_status(status, body):
    try:
        value = json.loads(body)
        if not isinstance(value, dict):
            return False
        if status == 200:
            return value.get("status") == "running"
        if status == 401:
            return value.get("status") == "failed" and value.get("message") in (
                "Unauthorized", "Token expired or invalid")
        if status == 403:
            return value.get("status") == "failed" and (
                value.get("consentRequired") is True or value.get("mustChangePassword") is True)
        return False
    except (ValueError, UnicodeError):
        return False


async def is_snowluma_http(listener):
    async def exchange():
        reader, writer = await asyncio.open_connection(listener["host"], listener["port"], limit=8192)
        try:
            host = listener["host"]
            authority = f"[{host}]" if ":" in host else host
            writer.write(f"GET /api/status HTTP/1.1\r\nHost: {authority}:{listener['port']}\r\nConnection: close\r\n\r\n".encode("ascii"))
            await writer.drain()
            headers = await reader.readuntil(b"\r\n\r\n")
            if len(headers) > 8192:
                return False
            lines = headers.decode("iso-8859-1").split("\r\n")
            status = int(lines[0].split()[1])
            fields = {}
            for line in lines[1:]:
                if line:
                    name, value = line.split(":", 1)
                    fields[name.strip().lower()] = value.strip()
            body = bytearray()
            if fields.get("transfer-encoding", "").lower() == "chunked":
                while True:
                    chunk_size = int((await reader.readline()).split(b";", 1)[0].strip(), 16)
                    if chunk_size < 0 or len(body) + chunk_size > 4096:
                        return False
                    if chunk_size == 0:
                        break
                    body.extend(await reader.readexactly(chunk_size))
                    if await reader.readexactly(2) != b"\r\n":
                        return False
            elif "content-length" in fields:
                size = int(fields["content-length"])
                if size < 0 or size > 4096:
                    return False
                body.extend(await reader.readexactly(size))
            else:
                while True:
                    chunk = await reader.read(4097 - len(body))
                    body.extend(chunk)
                    if len(body) > 4096:
                        return False
                    if not chunk:
                        break
            return valid_http_status(status, body)
        finally:
            writer.close()
    try:
        return await asyncio.wait_for(exchange(), timeout=1)
    except (OSError, ValueError, UnicodeError, IndexError, asyncio.IncompleteReadError,
            asyncio.LimitOverrunError, asyncio.TimeoutError):
        return False


async def check_http_candidates(listeners):
    limit = asyncio.Semaphore(2)
    async def check(listener):
        async with limit:
            return await is_snowluma_http(listener)
    return await asyncio.gather(*(check(listener) for listener in listeners))


def inspect_endpoint(candidates, expected_uid, node_exe, sl_dir, proc_root=Path("/proc")):
    result = inspect_candidates(candidates, expected_uid, node_exe, sl_dir, proc_root, endpoint=True)
    if result["state"] != "present":
        return result
    node = result["node"]
    try:
        listeners = listening_sockets(node, proc_root)
        if not listeners:
            raise ProbeError("not_listening", "已确认 SnowLuma Node 未监听 HTTP 端口")
        matches = asyncio.run(check_http_candidates(listeners))
        verified = inspect_one(node, expected_uid, node_exe, sl_dir, proc_root, endpoint=True)
        if verified != node or listening_sockets(node, proc_root) != listeners:
            raise ProbeError("changed", "Node 身份或监听端口在 HTTP 验证期间发生变化")
        matched_listeners = [listener for listener, matched in zip(listeners, matches) if matched]
        ports = {listener["port"] for listener in matched_listeners}
        if len(ports) > 1:
            raise ProbeError("conflict", "同一 SnowLuma Node 存在多个 WebUI HTTP 端口，请检查重复监听")
        if not ports:
            raise ProbeError("unreachable", "SnowLuma Node 的监听端口未响应有效 WebUI HTTP")
        selected = min(matched_listeners, key=lambda listener: (
            0 if listener["host"] == "127.0.0.1" else 1 if listener["host"] == "::1" else 2,
            listener["host"],
        ))
        result.update(port=ports.pop(), host=selected["host"], listeners=listeners)
        return result
    except ProbeError as error:
        if error.code == "permission_denied":
            return {"state": "permission_denied", "snapshot": candidates}
        raise


def probe(mode, expected_uid, node_exe, sl_dir, candidates=None, proc_root=Path("/proc")):
    if not posixpath.isabs(node_exe) or not posixpath.isabs(sl_dir):
        raise ProbeError("invalid", "Node 安装路径必须为绝对路径")
    node_exe = os.path.realpath(node_exe)
    sl_dir = os.path.realpath(sl_dir)
    if mode in ("inspect", "endpoint_inspect"):
        if not candidates:
            raise ProbeError("invalid", "只读提权必须绑定明确的 Node PID")
        inspect = inspect_endpoint if mode == "endpoint_inspect" else inspect_candidates
        return inspect(candidates, expected_uid, node_exe, sl_dir, proc_root)
    if os.getuid() != expected_uid:
        raise ProbeError("invalid", "Node 候选枚举必须使用原 SSH 用户")
    before = snapshot(expected_uid, proc_root)
    if mode == "snapshot":
        return {"state": "snapshot", "snapshot": before}
    if mode not in ("scan", "endpoint_scan"):
        raise ProbeError("invalid", "Node 检查模式无效")
    inspect = inspect_endpoint if mode == "endpoint_scan" else inspect_candidates
    result = inspect(before, expected_uid, node_exe, sl_dir, proc_root)
    if snapshot(expected_uid, proc_root) != before:
        raise ProbeError("changed", "Node 候选进程在检查期间变化，未启动新实例")
    return result


def main():
    try:
        mode, uid, node_exe, sl_dir, *rest = sys.argv[1:]
        candidates = json.loads(rest[0]) if rest else None
        result = probe(mode, int(uid), node_exe, sl_dir, candidates)
        print(json.dumps(result))
    except ProbeError as error:
        print(json.dumps({"state": "error", "code": error.code, "message": str(error)}))
        return 1
    except Exception:
        print(json.dumps({"state": "error", "code": "unknown",
                          "message": "Node 只读身份检查失败，未启动新实例"}))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

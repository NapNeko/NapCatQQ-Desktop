"""Resolve an account's QQ main process from one bounded process snapshot."""

import asyncio
import concurrent.futures
import json
import os
import re
import ssl
import sys
from dataclasses import dataclass


@dataclass(frozen=True)
class Process:
    pid: int
    birth: str
    argv: tuple
    exe: object


def read_limited(path, limit):
    with open(path, "rb") as stream:
        value = stream.read(limit + 1)
    if len(value) > limit:
        raise ValueError("process metadata exceeds limit")
    return value


def birth(pid):
    value = read_limited(f"/proc/{pid}/stat", 8192).decode()
    return value.rsplit(") ", 1)[1].split()[19]


def inspect(pid, qq_bin):
    try:
        comm = read_limited(f"/proc/{pid}/comm", 256).decode("utf-8", "replace").strip()
        if comm not in ("qq", "QQ"):
            return None
        started = birth(pid)
        argv = tuple(
            value.decode("utf-8", "replace")
            for value in read_limited(f"/proc/{pid}/cmdline", 32768).split(b"\0")
            if value
        )
        if not argv or any(value.startswith("--type=") for value in argv):
            return None
        try:
            exe = os.path.realpath(os.readlink(f"/proc/{pid}/exe"))
        except PermissionError:
            # Imported systemd QQ may use another UID while exposing cmdline/stat.
            exe = None
        if qq_bin:
            expected = os.path.realpath(qq_bin)
            actual = exe
            if actual is None:
                if not os.path.isabs(argv[0]):
                    raise ValueError("QQ executable identity is unavailable")
                actual = os.path.realpath(argv[0])
            if actual != expected:
                return None
        if birth(pid) != started:
            return None
        return Process(pid, started, argv, exe)
    except (FileNotFoundError, ProcessLookupError):
        return None


def snapshot(qq_bin):
    found = []
    unknown = False
    with os.scandir("/proc") as entries:
        for index, entry in enumerate(entries):
            if index >= 32768:
                raise ValueError("too many process entries")
            if not entry.name.isdecimal():
                continue
            try:
                process = inspect(int(entry.name), qq_bin)
                if process is not None:
                    found.append(process)
            except (OSError, ValueError, IndexError):
                unknown = True
    if len(found) > 256:
        raise ValueError("too many QQ main processes")
    return found, unknown


def accounts(process):
    result = set()
    args = process.argv[1:]
    for index, value in enumerate(args):
        if value in ("-q", "--qq", "--qq-id"):
            if index + 1 < len(args) and args[index + 1].isdecimal():
                result.add(args[index + 1])
        elif value.startswith(("--qq=", "--qq-id=")):
            account = value.split("=", 1)[1]
            if account.isdecimal():
                result.add(account)
        elif value.startswith("-q") and value[2:].isdecimal():
            result.add(value[2:])
    return result


def still_current(process, qq_bin):
    current = inspect(process.pid, qq_bin)
    return (
        current is not None
        and current.birth == process.birth
        and current.argv == process.argv
        and (process.exe is None or current.exe == process.exe)
    )


def owned_ptlogin_ports(process):
    owned = set()
    with os.scandir(f"/proc/{process.pid}/fd") as descriptors:
        for index, descriptor in enumerate(descriptors):
            if index >= 4096:
                raise ValueError("too many QQ descriptors")
            try:
                target = os.readlink(descriptor.path)
            except FileNotFoundError:
                continue
            match = re.fullmatch(r"socket:\[(\d+)\]", target)
            if match:
                owned.add(match.group(1))
    ports = set()
    for table in ("tcp", "tcp6"):
        text = read_limited(f"/proc/{process.pid}/net/{table}", 2097152).decode()
        for line in text.splitlines()[1:]:
            fields = line.split()
            if len(fields) > 9 and fields[3] == "0A" and fields[9] in owned:
                port = int(fields[1].rsplit(":", 1)[1], 16)
                if port in (4301, 4303, 4305, 4307, 4309):
                    ports.add(port)
    return ports


def response_accounts(value):
    pending = [value]
    result = set()
    for _ in range(512):
        if not pending:
            return result
        item = pending.pop()
        if isinstance(item, dict):
            account = item.get("uin")
            if isinstance(account, (str, int)) and str(account).isdecimal():
                result.add(str(account))
            pending.extend(item.values())
        elif isinstance(item, list):
            pending.extend(item)
    raise ValueError("too many account entries")


async def ptlogin_exchange(port, secure):
    options = {"limit": 8192}
    if secure:
        options.update(ssl=ssl._create_unverified_context(), server_hostname="localhost.ptlogin2.qq.com")
    reader, writer = await asyncio.open_connection("127.0.0.1", port, **options)
    try:
        # Direct local sockets bypass environment proxies and never follow redirects.
        writer.write(
            b"GET /pt_get_uins?callback=ptui_getuins_CB&pt_local_tk=0 HTTP/1.1\r\n"
            b"Host: localhost.ptlogin2.qq.com\r\n"
            b"Referer: https://xui.ptlogin2.qq.com/\r\n"
            b"Cookie: pt_local_token=0\r\nConnection: close\r\n\r\n"
        )
        await writer.drain()
        header = await reader.readuntil(b"\r\n\r\n")
        if len(header) > 8192 or header.split(b"\r\n", 1)[0].split()[1] != b"200":
            return set()
        headers = {}
        for line in header.split(b"\r\n")[1:]:
            if b":" in line:
                key, value = line.split(b":", 1)
                headers[key.strip().lower()] = value.strip().lower()
        body = bytearray()
        if headers.get(b"transfer-encoding") == b"chunked":
            while True:
                size = int((await reader.readline()).split(b";", 1)[0].strip(), 16)
                if size == 0:
                    break
                if size < 0 or len(body) + size > 65536:
                    raise ValueError("account response exceeds limit")
                body.extend(await reader.readexactly(size))
                if await reader.readexactly(2) != b"\r\n":
                    raise ValueError("invalid account response")
        elif b"content-length" in headers:
            length = int(headers[b"content-length"])
            if length < 0 or length > 65536:
                raise ValueError("account response exceeds limit")
            body.extend(await reader.readexactly(length))
        else:
            while True:
                chunk = await reader.read(min(4096, 65537 - len(body)))
                if not chunk:
                    break
                body.extend(chunk)
                if len(body) > 65536:
                    raise ValueError("account response exceeds limit")
        payload = body.decode("utf-8", "replace").strip()
        if not payload.startswith(("{", "[")):
            payload = payload[payload.index("(") + 1 : payload.rindex(")")]
        return response_accounts(json.loads(payload))
    finally:
        writer.close()


async def ptlogin_probe(port):
    for secure in (True, False):
        try:
            result = await asyncio.wait_for(ptlogin_exchange(port, secure), timeout=0.35)
            if result:
                return result
        except (
            OSError, ValueError, IndexError, RecursionError, asyncio.TimeoutError,
            asyncio.IncompleteReadError, asyncio.LimitOverrunError,
        ):
            pass
    return set()


def ptlogin_accounts(port):
    async def bounded():
        try:
            return await asyncio.wait_for(ptlogin_probe(port), timeout=0.8)
        except asyncio.TimeoutError:
            return set()

    return asyncio.run(bounded())


def select(qq_id, processes, allow_single, qq_bin):
    exact = [process for process in processes if accounts(process) == {qq_id}]
    if not exact:
        exact = [
            process for process in processes
            if not accounts(process) and qq_id in process.argv[1:]
        ]
    if len(exact) > 1:
        raise ValueError("multiple QQ processes claim the account")
    if exact:
        return exact[0]
    if allow_single and len(processes) == 1 and not accounts(processes[0]):
        return processes[0]
    if not processes:
        return None
    ownership = {}
    for process in processes:
        if accounts(process):
            continue
        for port in owned_ptlogin_ports(process):
            ownership.setdefault(port, []).append(process)
    matched = set()
    with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
        futures = {pool.submit(ptlogin_accounts, port): (port, owners) for port, owners in ownership.items()}
        for future, (port, owners) in futures.items():
            if qq_id in future.result():
                for process in owners:
                    if port not in owned_ptlogin_ports(process) or not still_current(process, qq_bin):
                        raise ValueError("QQ listener ownership changed during observation")
                    matched.add(process.pid)
    if len(matched) > 1:
        raise ValueError("multiple QQ processes claim the account")
    if matched:
        return next(process for process in processes if process.pid in matched)
    return None


def main(argv):
    qq_id, _pid_file, qq_bin, allow_single = argv
    processes, unknown = snapshot(qq_bin)
    # A PID file alone never establishes account ownership, even when still alive.
    process = select(qq_id, processes, allow_single == "1" and not unknown, qq_bin)
    if process is not None:
        if not still_current(process, qq_bin):
            raise ValueError("QQ identity changed during observation")
        print(process.pid)
    elif unknown:
        raise ValueError("some QQ process identities could not be inspected")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (OSError, ValueError, IndexError):
        print("QQ process ownership could not be confirmed", file=sys.stderr)
        sys.exit(2)

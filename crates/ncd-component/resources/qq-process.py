"""Inspect or stop one attributed host QQ process without scanning."""

import json
import os
import re
import select
import signal
import subprocess
import sys


class Replaced(Exception):
    pass


class ManagedService(Exception):
    def __init__(self, unit):
        self.unit = unit


def read_limited(path, limit):
    with open(path, "rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("metadata_limit")
    return data


def process_birth(pid):
    fields = read_limited(f"/proc/{pid}/stat", 8192).decode().rsplit(") ", 1)[1].split()
    if fields[0] in ("Z", "X"):
        raise ProcessLookupError()
    return fields[19]


def process_metadata(pid):
    value = read_limited(f"/proc/{pid}/stat", 8192).decode()
    fields = value.rsplit(") ", 1)[1].split()
    return fields[19], int(fields[1]), value[value.index("(") + 1 : value.rindex(")")]


def account_args(args):
    accounts = set()
    for index, arg in enumerate(args):
        value = None
        if arg in ("-q", "--qq", "--qq-id"):
            value = args[index + 1] if index + 1 < len(args) else None
        elif arg.startswith(("--qq=", "--qq-id=")):
            value = arg.split("=", 1)[1]
        elif arg.startswith("-q"):
            value = arg[2:]
        if value is not None and value.isdecimal():
            accounts.add(value)
    return accounts


def require_uid(request):
    expected = request.get("expected_uid", os.getuid())
    if os.getuid() not in (0, expected):
        raise Replaced()
    path = f"/proc/{request['pid']}"
    # Non-dumpable processes may expose root-owned proc entries despite an unchanged UID.
    if os.stat(path).st_uid not in (0, expected):
        raise Replaced()
    lines = read_limited(path + "/status", 65536).decode().splitlines()
    fields = next((line.split()[1:] for line in lines if line.startswith("Uid:")), [])
    if fields != [str(expected)] * 4:
        raise Replaced()


def inspect(request, snapshot=False):
    pid = request["pid"]
    if process_birth(pid) != request["start_time"]:
        raise Replaced()
    require_uid(request)
    args = [
        value.decode("utf-8", "replace")
        for value in read_limited(f"/proc/{pid}/cmdline", 32768).split(b"\0")
        if value
    ]
    if not args or any(arg == "--type" or arg.startswith("--type=") for arg in args[1:]):
        raise Replaced()
    expected = os.path.realpath(request["qq_bin"])
    if not os.path.isabs(args[0]) or os.path.realpath(args[0]) != expected:
        raise Replaced()
    if not snapshot and os.path.realpath(os.readlink(f"/proc/{pid}/exe")) != expected:
        raise Replaced()
    accounts = account_args(args[1:])
    if accounts and accounts != {str(request["qq_id"])}:
        raise Replaced()
    require_uid(request)
    if process_birth(pid) != request["start_time"]:
        raise Replaced()


def systemctl_path():
    for path in ("/usr/bin/systemctl", "/bin/systemctl"):
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    raise FileNotFoundError("systemctl_unavailable")


def managed_service(pid):
    text = read_limited(f"/proc/{pid}/cgroup", 32768).decode("utf-8", "replace")
    units = set()
    for line in text.splitlines():
        fields = line.split(":", 2)
        if len(fields) != 3:
            continue
        unit = fields[2].rstrip("/").rsplit("/", 1)[-1]
        if re.fullmatch(r"[A-Za-z0-9_.@:\\-]{1,240}\.service", unit):
            units.add(unit)
    if len(units) > 1:
        raise ValueError("ambiguous_service_owner")
    unit = next(iter(units), None)
    if unit is None:
        return None
    try:
        result = subprocess.run(
            [systemctl_path(), "show", "--property=MainPID,Restart", "--", unit],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
            timeout=2, check=False,
        )
    except subprocess.TimeoutExpired:
        raise ValueError("service_owner_timeout") from None
    if result.returncode != 0 or len(result.stdout) > 2048:
        raise ValueError("service_owner_unknown")
    properties = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
    main_pid = int(properties.get("MainPID", "0"))
    if not main_pid or properties.get("Restart", "no") == "no":
        return None
    main_birth, _, main_comm = process_metadata(main_pid)
    # SSH/systemd session ancestors do not turn QQ termination into unit failure.
    wrapper = main_comm in ("qq", "QQ", "sh", "bash", "dash", "zsh", "ksh", "node", "nodejs")
    wrapper = wrapper or main_comm.startswith(("python", "dbus-run-sess"))
    if not wrapper:
        return None
    current = pid
    visited = set()
    for _ in range(32):
        if current in visited or current <= 1:
            break
        visited.add(current)
        if current == main_pid:
            if process_metadata(main_pid)[0] != main_birth:
                raise Replaced()
            return unit
        _, current, _ = process_metadata(current)
    return None


def stop(request):
    inspect(request)
    unit = managed_service(request["pid"])
    if unit:
        raise ManagedService(unit)
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        raise ValueError("pidfd_unavailable")
    descriptor = os.pidfd_open(request["pid"], 0)
    try:
        # The descriptor pins the original process even if its numeric PID is reused.
        inspect(request)
        signal.pidfd_send_signal(descriptor, signal.SIGTERM, None, 0)
        if select.select([descriptor], [], [], 5)[0]:
            return
        inspect(request)
        signal.pidfd_send_signal(descriptor, signal.SIGKILL, None, 0)
        if not select.select([descriptor], [], [], 2)[0]:
            raise ValueError("qq_stop_timeout")
    finally:
        os.close(descriptor)


def validate(request):
    return (
        isinstance(request, dict)
        and request.get("v") == 1
        and request.get("action") in ("inspect", "snapshot", "stop")
        and type(request.get("pid")) is int
        and 0 < request["pid"] <= 2147483647
        and isinstance(request.get("start_time"), str)
        and request["start_time"].isascii()
        and request["start_time"].isdecimal()
        and len(request["start_time"]) <= 32
        and type(request.get("qq_id")) is int
        and request["qq_id"] > 0
        and isinstance(request.get("qq_bin"), str)
        and request["qq_bin"].startswith("/")
        and len(request["qq_bin"]) <= 4096
        and "\0" not in request["qq_bin"]
        and ("expected_uid" not in request or (
            type(request["expected_uid"]) is int and 0 <= request["expected_uid"] <= 4294967295
        ))
    )


def execute(request):
    reply = {"v": 1, "pid": request["pid"], "start_time": request["start_time"], "uid": os.getuid()}
    try:
        if request["action"] == "stop":
            stop(request)
            reply["state"] = "stopped"
        else:
            inspect(request, request["action"] == "snapshot")
            reply["state"] = "verified"
    except (FileNotFoundError, ProcessLookupError):
        # Missing supervisor metadata or binaries do not establish that the target exited.
        try:
            reply["state"] = "error" if process_birth(request["pid"]) == request["start_time"] else "replaced"
        except (FileNotFoundError, ProcessLookupError):
            reply["state"] = "absent"
        except PermissionError:
            reply["state"] = "permission"
        except (OSError, ValueError, IndexError):
            reply["state"] = "error"
    except Replaced:
        reply["state"] = "replaced"
    except ManagedService as owner:
        reply["state"] = "managed_service"
        reply["unit"] = owner.unit
    except PermissionError:
        reply["state"] = "permission"
    except (OSError, ValueError, IndexError):
        reply["state"] = "error"
    return reply


def main():
    raw = sys.stdin.buffer.read(8193)
    if len(raw) > 8192:
        raise ValueError("request_limit")
    request = json.loads(raw)
    if not validate(request):
        raise ValueError("invalid_request")
    print(json.dumps(execute(request), separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, IndexError):
        print("QQ process request rejected", file=sys.stderr)
        sys.exit(2)

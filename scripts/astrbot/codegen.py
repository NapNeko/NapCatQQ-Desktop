"""Generate the AstrBot fallback metadata and coverage inventory from a pinned source checkout.

Only literal AST nodes and the two known default factories are interpreted. No AstrBot
module is imported and no upstream code runs. Usage: python scripts/astrbot/codegen.py
--src /tmp/AstrBot --commit <full commit> --version 4.29.1
"""
from __future__ import annotations

import argparse
import ast
import copy
import hashlib
import json
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
UI = REPO / "src-ui/core/domain/apps/astrbotSchema"
RUST = REPO / "crates/ncd-appframework/src/astrbot/schema"

def assignment(path: Path, name: str):
    tree = ast.parse(path.read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == name for t in node.targets):
            return node.value
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id == name:
            return node.value
    raise ValueError(f"{path}: no assignment {name}")

def literal(node, env):
    if isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, ast.Name):
        if node.id not in env:
            raise ValueError(f"unexpected name: {node.id}")
        return copy.deepcopy(env[node.id])
    if isinstance(node, (ast.List, ast.Tuple, ast.Set)):
        return [literal(v, env) for v in node.elts]
    if isinstance(node, ast.Dict):
        out = {}
        for k, v in zip(node.keys, node.values):
            if k is None:
                out.update(literal(v, env))
            else:
                out[literal(k, env)] = literal(v, env)
        return out
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd)):
        value = literal(node.operand, env)
        return -value if isinstance(node.op, ast.USub) else value
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Add):
        return literal(node.left, env) + literal(node.right, env)
    if isinstance(node, ast.Subscript):
        return literal(node.value, env)[literal(node.slice, env)]
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and not node.keywords:
        args = [literal(a, env) for a in node.args]
        if node.func.id == "get_agent_runner_config_default" and len(args) == 1:
            return copy.deepcopy(env["AGENT_RUNNER_CONFIG_DEFAULTS"][args[0]])
        if node.func.id == "get_local_permission_defaults" and len(args) <= 1:
            return {
                "member": {"allow_execution": False, "allow_network": False, "filesystem_scope": "workspace"},
                "admin": {"allow_execution": True, "allow_network": True, "filesystem_scope": "host"},
            }
    if isinstance(node, ast.DictComp) and len(node.generators) == 1:
        gen = node.generators[0]
        if not isinstance(gen.target, ast.Name) or gen.ifs or gen.is_async:
            raise ValueError("unsupported comprehension")
        out = {}
        for value in literal(gen.iter, env):
            local = {**env, gen.target.id: value}
            out[literal(node.key, local)] = literal(node.value, local)
        return out
    raise ValueError(f"unsupported literal: {ast.dump(node)}")

def leaves(value, prefix=""):
    if isinstance(value, dict) and value:
        for key, child in value.items():
            yield from leaves(child, f"{prefix}.{key}" if prefix else key)
    else:
        yield prefix

def metadata_fields(metadata):
    for group in metadata.values():
        for section in group.get("metadata", {}).values():
            for path, node in section.get("items", {}).items():
                if isinstance(node, dict):
                    yield path

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", type=Path, required=True)
    ap.add_argument("--commit", required=True)
    ap.add_argument("--version", required=True)
    args = ap.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        raise ValueError("--commit must be a full git commit SHA")
    src = args.src.resolve()
    paths = {
        "defaults": "astrbot/core/config/default.py",
        "runners": "astrbot/core/config/agent_runner.py",
        "cua": "astrbot/core/computer/booters/cua_defaults.py",
        "translations": "dashboard/src/i18n/locales/zh-CN/features/config-metadata.json",
    }
    env = {}
    for key, name in (("runners", "AGENT_RUNNER_CONFIG_DEFAULTS"), ("cua", "CUA_DEFAULT_CONFIG")):
        env[name] = literal(assignment(src / paths[key], name), env)
    env["PERSONAL_WECHAT_CONFIG_METADATA"] = literal(assignment(src / paths["defaults"], "PERSONAL_WECHAT_CONFIG_METADATA"), env)
    env["WEBHOOK_SUPPORTED_PLATFORMS"] = literal(assignment(src / paths["defaults"], "WEBHOOK_SUPPORTED_PLATFORMS"), env)
    legacy_metadata = literal(assignment(src / paths["defaults"], "CONFIG_METADATA_2"), env)
    defaults = literal(assignment(src / paths["defaults"], "DEFAULT_CONFIG"), env)
    metadata = {
        **literal(assignment(src / paths["defaults"], "CONFIG_METADATA_3"), env),
        **literal(assignment(src / paths["defaults"], "CONFIG_METADATA_3_SYSTEM"), env),
    }
    translations = json.loads((src / paths["translations"]).read_text(encoding="utf-8"))
    rust = (REPO / "crates/ncd-appframework/src/astrbot/advanced.rs").read_text(encoding="utf-8")
    managed_match = re.search(r"MANAGED_PATHS:.*?= &\[(.*?)\];", rust, re.S)
    if not managed_match:
        raise ValueError("cannot find MANAGED_PATHS")
    managed = re.findall(r'"([^"]+)"', managed_match.group(1))
    source = {
        "repository": "AstrBotDevs/AstrBot",
        "commit": args.commit,
        "version": args.version,
        "files": {path: hashlib.sha256((src / path).read_bytes()).hexdigest() for path in paths.values()},
    }
    snapshot = {"source": source, "defaults": defaults, "metadata": metadata,
                "translations": translations, "managed_paths": managed}
    UI.mkdir(parents=True, exist_ok=True)
    (UI / "snapshot.json").write_text(json.dumps(snapshot, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    fields = sorted(set(leaves(defaults)) | set(metadata_fields(metadata)))
    dedicated = lambda path: any(path == own or path.startswith(own + ".") for own in managed)
    report = {
        "source": source,
        "inventory": [{"path": path, "surface": "dedicated" if dedicated(path) else "advanced"} for path in fields],
        "template_inventory": [
            {"kind": field, "template": name, "paths": sorted(leaves(template)), "surface": "dedicated_or_raw"}
            for field, spec in {
                **legacy_metadata.get("provider_group", {}).get("metadata", {}),
                **legacy_metadata.get("platform_group", {}).get("metadata", {}),
            }.items()
            for name, template in spec.get("config_template", {}).items()
        ],
        "provider_parameters": "Provider-specific templates remain available through the existing raw cmd_config.json editor; unknown provider keys are preserved by typed saves.",
        "live_metadata": ["/api/config/default", "/api/config/abconf?system_config=1"],
        "offline_metadata": "snapshot.json",
    }
    RUST.mkdir(parents=True, exist_ok=True)
    (RUST / "coverage.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"AstrBot {args.version}: {len(fields)} config paths, {len(metadata)} metadata groups")

if __name__ == "__main__":
    main()

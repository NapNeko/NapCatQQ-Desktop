"""从 MaiBot 源码生成 Desktop 用的配置类型。

MaiBot 的 bot_config / model_config 由 pydantic 类定义、没有模板文件，字段有五百多个，
手抄 Rust 结构体和界面文案必然抄错。这里直接内省上游的类：

- crates/ncd-appframework/src/maibot/schema/generated.rs   强类型结构体 + 范围 / 选项校验
- crates/ncd-appframework/src/maibot/schema/defaults/*.toml 上游自己生成的完整默认配置（读取时垫底）
- src-ui/core/domain/apps/maibotSchema/*.json              界面用的中文标签、说明、控件、分组、范围

用法（要一个装好依赖的 MaiBot venv；Desktop 装的实例目录里就有）：

    <实例>/.venv/Scripts/python.exe scripts/maibot/codegen.py --src <MaiBot 源码目录>

在临时目录中通过 AST 白名单仅载入上游配置模型及纯数据导出函数，不运行框架启动、数据库迁移或网络逻辑。\n只需 Python 3.12+ 与 pydantic / tomlkit，无需安装 MaiBot 业务依赖。
升级上游 MaiBot 后重跑一遍，看 git diff 决定要不要跟进界面和校验。
"""

from __future__ import annotations

import argparse
import ast
import json
import os
import shutil
import subprocess
import tomllib
import sys
import tempfile
import types
import typing
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
RUST_OUT = REPO / "crates/ncd-appframework/src/maibot/schema/generated.rs"
DEFAULTS_OUT = REPO / "crates/ncd-appframework/src/maibot/schema/defaults"
UI_OUT = REPO / "src-ui/core/domain/apps/maibotSchema"
TS_EXPORT = "../../../src-ui/core/ipc/generated/maibot/"
SKIP_FIELDS = {"field_docs", "_validate_any", "suppress_any_warning"}
# 上游 dict[str, Any] 字段：界面上当行内 TOML 文本编辑，读写时由 schema/mod.rs 按路径转换
ANY_TABLE_FIELDS = {"ModelInfo.extra_params"}
# 界面用得上的 schema 键；其余（多语言标签、布局宽度之类）丢掉。默认值留着：列表里「加一条」要按它起新条目
UI_KEYS = (
    "name", "type", "label", "description", "default", "options", "minValue", "maxValue",
    "step", "x-widget", "x-option-labels", "x-option-descriptions", "advanced", "x-row",
    "placeholder", "items", "x-textarea-rows", "x-description-display", "hidden",
    "exclusiveMinValue", "exclusiveMaxValue", "minLength", "maxLength", "pattern",
    "mapValues", "secret",
)
UI_NODE_KEYS = ("className", "uiLabel", "uiAdvanced", "uiOrder", "uiParent")


# 两份文件的根按文件起名：根类 Config 和 [bot] 节的 BotConfig 按通用规则会撞成同一个名字
ROOT_NAMES = {"Config": "MaiBotBotConfigFile", "ModelConfig": "MaiBotModelConfigFile"}


def rust_name(cls_name: str) -> str:
    if cls_name in ROOT_NAMES:
        return ROOT_NAMES[cls_name]
    if cls_name.startswith("MaiBot"):
        return cls_name
    return "MaiBot" + cls_name


RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
    "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override",
    "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
}


def ident(name: str) -> str:
    """上游字段名撞 Rust 关键字（学习规则的 type / use）就写成原始标识符，serde 序列化时会去掉 r#"""
    return f"r#{name}" if name in RUST_KEYWORDS else name


def rust_str(text: str) -> str:
    """Rust 字符串字面量。控制字符按 Rust 的花括号 unicode 转义写，Rust 不认 JSON 的四位写法"""
    escapes = {"\\": "\\\\", '"': '\\"', "\n": "\\n", "\r": "\\r", "\t": "\\t"}
    out = []
    for ch in text:
        if ch in escapes:
            out.append(escapes[ch])
        elif ord(ch) < 0x20 or ord(ch) == 0x7F:
            out.append("\\u{%x}" % ord(ch))
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def inline_toml(value: dict) -> str:
    import tomlkit

    table = tomlkit.inline_table()
    table.update(value)
    return table.as_string()


class Gen:
    def __init__(self, config_base, schema_gen):
        self.ConfigBase = config_base
        self.schema_gen = schema_gen
        self.order: list[type] = []
        self.classes: dict[str, type] = {}
        self.override: dict[str, bool] = {}

    def is_cfg(self, t) -> bool:
        return isinstance(t, type) and issubclass(t, self.ConfigBase)

    def collect(self, cls, override_repr: bool):
        # 安全导出没有框架首次初始化；在类所在模块已完整载入后显式解析前向引用。
        cls.model_rebuild(force=True)
        seen = self.classes.get(cls.__name__)
        if seen is not None:
            # 按类名去重：两个不同的类撞名会生成成同一个结构体，得先改名规则
            if seen is not cls:
                raise SystemExit(f"类名冲突：{cls.__module__}.{cls.__name__}")
            return
        self.classes[cls.__name__] = cls
        self.override[cls.__name__] = override_repr
        for name, info in cls.model_fields.items():
            if name in SKIP_FIELDS or (not info.repr and not override_repr):
                continue
            for inner in self.walk_types(info.annotation):
                if self.is_cfg(inner):
                    self.collect(inner, override_repr)
        self.order.append(cls)

    def walk_types(self, t):
        yield t
        for arg in typing.get_args(t):
            yield from self.walk_types(arg)

    # ---- 类型映射 ----

    def rust_type(self, t, path: str) -> tuple[str, dict]:
        """返回 (Rust 类型, 附加信息)；附加信息里 literal = 选项，ts = 覆盖的 TS 类型"""
        origin = typing.get_origin(t)
        args = typing.get_args(t)
        if origin in (typing.Union, getattr(types, "UnionType", None)):
            rest = [a for a in args if a is not type(None)]
            if len(rest) != 1 or len(rest) == len(args):
                raise SystemExit(f"{path}: 不支持的联合类型 {t!r}")
            inner, extra = self.rust_type(rest[0], path)
            return f"Option<{inner}>", {**extra, "optional": True}
        if origin is typing.Literal:
            if not all(isinstance(a, str) for a in args):
                raise SystemExit(f"{path}: 只支持字符串 Literal {t!r}")
            return "String", {"literal": list(args), "ts": " | ".join(json.dumps(a) for a in args)}
        if origin in (list, set, frozenset) or t in (list, set):
            (item,) = args or (str,)
            inner, extra = self.rust_type(item, path)
            ts = f"Array<{extra['ts']}>" if "ts" in extra else None
            return f"Vec<{inner}>", ({"ts": ts} if ts else {})
        if origin is dict:
            key, val = args
            if key is not str:
                raise SystemExit(f"{path}: 字典键必须是 str {t!r}")
            if val is typing.Any:
                return "String", {"any_table": True}
            inner, extra = self.rust_type(val, path)
            ts = f"Record<string, {extra.get('ts', 'string' if inner == 'String' else inner)}>"
            return f"std::collections::BTreeMap<String, {inner}>", {"ts": ts if 'ts' in extra else None}
        if self.is_cfg(t):
            return rust_name(t.__name__), {}
        if t is bool:
            return "bool", {}
        if t is int:
            return "i64", {"ts": "number"}
        if t is float:
            return "f64", {"float": True}
        if t is str:
            return "String", {}
        raise SystemExit(f"{path}: 不认识的类型 {t!r}")

    def field_schema(self, cls) -> dict[str, dict]:
        schema = self.schema_gen.generate_schema(cls, include_nested=False)
        fields = {f["name"]: f for f in schema.get("fields", [])}
        for name, info in cls.model_fields.items():
            if name in fields:
                self.add_constraints(fields[name], info)
        return fields

    @staticmethod
    def add_constraints(field: dict, info) -> None:
        # 上游 WebUI 目前只导出 ge/le；保存时 pydantic 同样检查开区间、长度和正则。
        for constraint in info.metadata:
            for attr, key in (
                ("gt", "exclusiveMinValue"), ("lt", "exclusiveMaxValue"),
                ("min_length", "minLength"), ("max_length", "maxLength"),
                ("pattern", "pattern"),
            ):
                value = getattr(constraint, attr, None)
                if value is not None:
                    field[key] = value
        if not info.repr:
            field["secret"] = True

    def ui_schema(self, cls) -> dict:
        schema = self.schema_gen.generate_config_schema(cls, include_nested=True)

        def enrich(node, model):
            nested = node.setdefault("nested", {})
            for field in node.get("fields", []):
                info = model.model_fields.get(field["name"])
                if info is None:
                    continue
                self.add_constraints(field, info)
                annotation = info.annotation
                origin, args = typing.get_origin(annotation), typing.get_args(annotation)
                if origin in (typing.Union, getattr(types, "UnionType", None)):
                    annotation = next((a for a in args if a is not type(None)), annotation)
                    origin, args = typing.get_origin(annotation), typing.get_args(annotation)
                inner = annotation
                if origin in (list, set, frozenset, tuple):
                    inner = args[0] if args else None
                elif origin is dict:
                    inner = args[1] if len(args) > 1 else None
                    if self.is_cfg(inner):
                        field["mapValues"] = True
                        nested[field["name"]] = self.schema_gen.generate_config_schema(inner, include_nested=True)
                if self.is_cfg(inner) and field["name"] in nested:
                    enrich(nested[field["name"]], inner)

        enrich(schema, cls)
        return trim_ui(schema)

    # ---- Rust 输出 ----

    def emit_rust(self, version: str) -> str:
        out = [
            f"// 由 scripts/maibot/codegen.py 从 MaiBot {version} 生成，别手改；升级上游后重跑。",
            "// 字段名、类型照上游 pydantic 类；默认值不在这里，读取时垫 defaults/ 下的上游默认文件。",
            "",
            # 默认值一律照上游显式写出，哪怕恰好全是零值
            "#![allow(clippy::derivable_impls)]",
            "",
            "use serde::{Deserialize, Serialize};",
            "use ts_rs::TS;",
            "",
            "use super::IssueSink;",
            "",
        ]
        for cls in self.order:
            out.extend(self.emit_struct(cls))
        return "\n".join(out) + "\n"

    def emit_struct(self, cls) -> list[str]:
        override_repr = self.override[cls.__name__]
        schema = self.field_schema(cls)
        name = rust_name(cls.__name__)
        lines = [
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]",
            # 缺的键按上游默认补：节、嵌套表、数组条目都一样，和 pydantic 按缺省读的结果一致
            "#[serde(default)]",
            f'#[ts(export, export_to = "{TS_EXPORT}")]',
            f"pub struct {name} {{",
        ]
        checks: list[str] = []
        defaults: list[str] = []
        instance = self.default_instance(cls)
        for fname, info in cls.model_fields.items():
            if fname in SKIP_FIELDS or (not info.repr and not override_repr):
                continue
            path = f"{cls.__name__}.{fname}"
            rtype, extra = self.rust_type(info.annotation, path)
            desc = (schema.get(fname, {}).get("description") or "").strip().splitlines()
            if desc and desc[0]:
                lines.append(f"    /// {desc[0]}")
            attrs = []
            if extra.get("optional"):
                attrs.append('#[serde(default, skip_serializing_if = "Option::is_none")]')
                attrs.append("#[ts(optional)]")
            if extra.get("any_table") and path not in ANY_TABLE_FIELDS:
                # 自由字典在 schema/mod.rs 里按路径转成文本再读，新冒出来的得先去那里补上
                raise SystemExit(f"{path}: 新的 dict[str, Any] 字段，先在 schema/mod.rs 的预处理里加上")
            ts = extra.get("ts")
            if ts and ts != "number" or (ts == "number" and "i64" in rtype):
                ts_decl = ts if not extra.get("optional") else ts
                attrs.append(f"#[ts(type = {json.dumps(ts_decl)})]")
            lines.extend("    " + a for a in attrs)
            lines.append(f"    pub {ident(fname)}: {rtype},")
            checks.extend(self.emit_checks(fname, rtype, extra, schema.get(fname, {})))
            value = self.default_of(cls, instance, fname, info)
            defaults.append(f"            {ident(fname)}: {self.lit(value, info.annotation, path)},")
        lines.append("}")
        lines.append("")
        lines.append(f"impl {name} {{")
        lines.append("    pub fn validate(&self, path: &str, sink: &mut IssueSink) {")
        if checks:
            lines.extend("        " + c for c in checks)
        else:
            lines.append("        let _ = (path, sink);")
        lines.append("    }")
        lines.append("}")
        lines.append("")
        lines.append(f"impl Default for {name} {{")
        lines.append("    fn default() -> Self {")
        lines.append("        Self {")
        lines.extend(defaults)
        lines.append("        }")
        lines.append("    }")
        lines.append("}")
        lines.append("")
        return lines

    # ---- 默认值 ----

    def default_instance(self, cls):
        """能无参构造就用上游实例（经过它自己的校验和归一）；有必填字段的条目类构造不了，逐字段取"""
        try:
            return cls()
        except Exception:
            return None

    def default_of(self, cls, instance, fname, info):
        if instance is not None:
            return getattr(instance, fname)
        from pydantic_core import PydanticUndefined
        value = info.get_default(call_default_factory=True)
        return None if value is PydanticUndefined else value

    def lit(self, value, t, path: str) -> str:
        origin = typing.get_origin(t)
        args = typing.get_args(t)
        if origin in (typing.Union, getattr(types, "UnionType", None)):
            (inner,) = [a for a in args if a is not type(None)]
            return "None" if value is None else f"Some({self.lit(value, inner, path)})"
        if origin is typing.Literal or t is str:
            return f"String::from({rust_str('' if value is None else str(value))})"
        if origin in (list, set, frozenset) or t in (list, set):
            (item,) = args or (str,)
            items = list(value or [])
            if origin in (set, frozenset) or t is set:
                items = sorted(items)
            return "vec![" + ", ".join(self.lit(v, item, path) for v in items) + "]"
        if origin is dict:
            _, val = args
            if val is typing.Any:
                return f"String::from({rust_str(inline_toml(value or {}))})"
            pairs = ", ".join(
                f"(String::from({rust_str(k)}), {self.lit(v, val, path)})" for k, v in (value or {}).items()
            )
            return f"std::collections::BTreeMap::from([{pairs}])" if pairs else "std::collections::BTreeMap::new()"
        if self.is_cfg(t):
            if value is None:
                return f"{rust_name(t.__name__)}::default()"
            override_repr = self.override[t.__name__]
            parts = []
            for fname, info in t.model_fields.items():
                if fname in SKIP_FIELDS or (not info.repr and not override_repr):
                    continue
                parts.append(f"{ident(fname)}: {self.lit(getattr(value, fname), info.annotation, path)}")
            return f"{rust_name(t.__name__)} {{ " + ", ".join(parts) + " }"
        if t is bool:
            return "true" if value else "false"
        if t is int:
            return str(int(value or 0))
        if t is float:
            f = float(value or 0.0)
            text = repr(f)
            return text if ("." in text or "e" in text) else text + ".0"
        raise SystemExit(f"{path}: 不会写默认值 {t!r} = {value!r}")

    def emit_checks(self, fname: str, rtype: str, extra: dict, fs: dict) -> list[str]:
        p = f'&format!("{{path}}/{fname}")'
        field = ident(fname)
        out = []
        lo, hi = fs.get("minValue"), fs.get("maxValue")
        numeric = rtype in ("i64", "f64")
        if numeric and (lo is not None or hi is not None):
            v = f"self.{field} as f64" if rtype == "i64" else f"self.{field}"
            lo_s = "None" if lo is None else f"Some({float(lo)!r})"
            hi_s = "None" if hi is None else f"Some({float(hi)!r})"
            out.append(f"sink.range({p}, {v}, {lo_s}, {hi_s});")
        if numeric:
            for key, comparison, message in (
                ("exclusiveMinValue", ">", "要大于"), ("exclusiveMaxValue", "<", "要小于"),
            ):
                if key in fs:
                    limit = float(fs[key])
                    out.append(f'if !((self.{field} as f64) {comparison} {limit!r}) {{ sink.push({p}, "{message} {limit:g}"); }}')
        if rtype == "String" and fs.get("pattern"):
            pattern = rust_str(str(fs["pattern"]))
            out.append(f'if !fancy_regex::Regex::new({pattern}).expect("上游配置正则").is_match(&self.{field}).unwrap_or(false) {{ sink.push({p}, "格式不正确"); }}')
        if rtype == "String" or rtype.startswith(("Vec<", "std::collections::BTreeMap<")):
            length = f"self.{field}.chars().count()" if rtype == "String" else f"self.{field}.len()"
            for key, comparison, message in (
                ("minLength", "<", "至少"), ("maxLength", ">", "最多"),
            ):
                if key in fs:
                    limit = int(fs[key])
                    out.append(f'if {length} {comparison} {limit} {{ sink.push({p}, "{message} {limit} 项"); }}')
        if "literal" in extra and rtype == "String":
            opts = ", ".join(json.dumps(o) for o in extra["literal"])
            out.append(f"sink.one_of({p}, &self.{field}, &[{opts}]);")
        if rtype.startswith("MaiBot"):
            out.append(f"self.{field}.validate({p}, sink);")
        elif rtype.startswith("Vec<MaiBot"):
            out.append(f"for (i, item) in self.{field}.iter().enumerate() {{")
            out.append(f'    item.validate(&format!("{{path}}/{fname}/{{i}}"), sink);')
            out.append("}")
        elif rtype.startswith("std::collections::BTreeMap<String, MaiBot"):
            out.append(f"for (k, item) in &self.{field} {{")
            out.append(f'    item.validate(&format!("{{path}}/{fname}/{{k}}"), sink);')
            out.append("}")
        return out


def trim_ui(node: dict) -> dict:
    out = {k: node[k] for k in UI_NODE_KEYS if k in node}
    fields = []
    for f in node.get("fields", []):
        g = {k: f[k] for k in UI_KEYS if k in f}
        if isinstance(g.get("label"), dict):
            g["label"] = g["label"].get("zh_CN") or next(iter(g["label"].values()), f["name"])
        fields.append(g)
    out["fields"] = fields
    nested = node.get("nested") or {}
    if nested:
        out["nested"] = {k: trim_ui(v) for k, v in nested.items()}
    return out


# 只运行配置模型及其纯数据导出函数，不导入 MaiBot 的启动、数据库或网络模块。
# AST 白名单在上游新增依赖时明确失败，避免升级时悄悄运行新的业务初始化。
SCHEMA_MODULES = (
    "src/config/config_base.py",
    "src/config/official_configs.py",
    "src/config/model_configs.py",
    "src/config/config_utils.py",
    "src/config/default_model_config.py",
    "src/agent/config.py",
    "src/web_search/config.py",
    "src/webui/config_schema.py",
    "src/config/config.py",
)
SCHEMA_IMPORTS = {
    "src.common.logger", "src.common.i18n", "src.config.config_base",
    "src.config.official_configs", "src.config.model_configs", "src.config.config_utils",
    "src.config.default_model_config", "src.agent.config", "src.web_search.config",
    "pydantic", "pydantic.fields", "pydantic_core", "tomlkit", "tomlkit.items",
}
ROOT_DECLARATIONS = {
    "Config", "ModelConfig", "CONFIG_VERSION", "MODEL_CONFIG_VERSION", "T", "logger",
    "generate_new_config_file", "write_config_to_file",
}


def prepare_schema_sources(src: Path, work: Path) -> None:
    def import_module(node, module):
        if isinstance(node, ast.Import):
            return [alias.name for alias in node.names]
        if not node.level:
            return [node.module or ""]
        package = module.split(".")[:-node.level]
        return [".".join(package + ([node.module] if node.module else []))]

    def allowed_import(name):
        return name.split(".")[0] in sys.stdlib_module_names or name in SCHEMA_IMPORTS

    for rel in SCHEMA_MODULES:
        source = src / rel
        tree = ast.parse(source.read_text(encoding="utf-8"), filename=str(source))
        module = rel.removesuffix(".py").replace("/", ".")
        selected = []
        class_names = {node.name for node in tree.body if isinstance(node, ast.ClassDef)}
        for node in tree.body:
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                names = import_module(node, module)
                if all(allowed_import(name) for name in names):
                    selected.append(node)
                elif rel != "src/config/config.py":
                    raise SystemExit(f"{rel}: 配置元数据新增未审查的依赖 {names!r}")
                continue
            if rel == "src/config/config.py":
                names = [node.name] if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) else [
                    target.id for target in (
                        node.targets if isinstance(node, ast.Assign) else
                        [node.target] if isinstance(node, ast.AnnAssign) else []
                    ) if isinstance(target, ast.Name)
                ]
                if not names or not all(name in ROOT_DECLARATIONS for name in names):
                    continue
            elif not isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef, ast.Assign, ast.AnnAssign, ast.Expr)):
                # TYPE_CHECKING 分支等不参与运行时配置元数据。
                continue
            if isinstance(node, ast.Expr) and not isinstance(node.value, ast.Constant):
                call = node.value
                schema_rebuild = (
                    isinstance(call, ast.Call) and isinstance(call.func, ast.Attribute)
                    and call.func.attr == "model_rebuild" and isinstance(call.func.value, ast.Name)
                    and call.func.value.id in class_names and not call.args and not call.keywords
                )
                if not schema_rebuild:
                    raise SystemExit(f"{rel}:{node.lineno}: 不允许配置导出时运行顶层表达式 {ast.dump(node.value)}")
            selected.append(node)
        target = work / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(ast.unparse(ast.Module(body=selected, type_ignores=[])) + "\n", encoding="utf-8")
        for parent in target.parents:
            if parent == work:
                break
            (parent / "__init__.py").touch()

    common = work / "src/common"
    common.mkdir(parents=True, exist_ok=True)
    (common / "__init__.py").touch()
    (common / "logger.py").write_text(
        "class _SchemaLogger:\n    def __getattr__(self, name):\n        return lambda *args, **kwargs: None\n"
        "def get_logger(*args, **kwargs):\n    return _SchemaLogger()\n", encoding="utf-8"
    )
    (common / "i18n.py").write_text(
        "def t(key, **kwargs):\n    return key\n", encoding="utf-8"
    )


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, help="MaiBot 源码根目录（含 src/ 与 pyproject.toml）")
    ap.add_argument("--revision", help="上游 commit；默认从源码仓库读取（归档输入须显式提供）")
    args = ap.parse_args()
    src = Path(args.src).resolve()
    revision = args.revision
    if not revision:
        result = subprocess.run(["git", "-C", str(src), "rev-parse", "HEAD"], capture_output=True, text=True, check=True)
        revision = result.stdout.strip()
    if len(revision) != 40 or any(ch not in "0123456789abcdef" for ch in revision):
        raise SystemExit("--revision 必须是完整的上游 commit SHA")
    project = tomllib.loads((src / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    work = Path(tempfile.mkdtemp(prefix="ncd-maibot-codegen-"))
    try:
        prepare_schema_sources(src, work)
        shutil.copy(src / "pyproject.toml", work / "pyproject.toml")
        os.chdir(work)
        sys.path.insert(0, str(work))
        sys.dont_write_bytecode = True
        # 只加载配置类，不触发 MaiBot 全局配置初始化或旧数据库迁移。
        os.environ["MAIBOT_INITIALIZE_ONLY"] = "1"

        from src.config.config import (  # noqa: E402
            CONFIG_VERSION, MODEL_CONFIG_VERSION, Config, ModelConfig, generate_new_config_file,
        )
        from src.config.config_base import ConfigBase  # noqa: E402
        from src.webui.config_schema import ConfigSchemaGenerator  # noqa: E402

        gen = Gen(ConfigBase, ConfigSchemaGenerator)
        gen.collect(Config, override_repr=False)
        gen.collect(ModelConfig, override_repr=True)

        version = f"bot_config {CONFIG_VERSION} / model_config {MODEL_CONFIG_VERSION}"
        RUST_OUT.parent.mkdir(parents=True, exist_ok=True)
        RUST_OUT.write_text(gen.emit_rust(version), encoding="utf-8", newline="\n")

        DEFAULTS_OUT.mkdir(parents=True, exist_ok=True)
        tmp_out = work / "out"
        tmp_out.mkdir()
        generate_new_config_file(Config, tmp_out / "bot_config.toml", CONFIG_VERSION)
        generate_new_config_file(
            ModelConfig, tmp_out / "model_config.toml", MODEL_CONFIG_VERSION, override_repr=True
        )
        for name in ("bot_config.toml", "model_config.toml"):
            text = (tmp_out / name).read_text(encoding="utf-8")
            (DEFAULTS_OUT / name).write_text(text, encoding="utf-8", newline="\n")

        UI_OUT.mkdir(parents=True, exist_ok=True)
        for name, cls in (("bot", Config), ("model", ModelConfig)):
            schema = gen.ui_schema(cls)
            (UI_OUT / f"{name}.json").write_text(
                json.dumps(schema, ensure_ascii=False, indent=1) + "\n",
                encoding="utf-8",
                newline="\n",
            )
        snapshot = {
            "repository": "https://github.com/Mai-with-u/MaiBot",
            "revision": revision,
            "projectVersion": project["version"],
            "botConfigVersion": CONFIG_VERSION,
            "modelConfigVersion": MODEL_CONFIG_VERSION,
            "classes": {
                cls.__name__: [
                    name for name, info in cls.model_fields.items()
                    if name not in SKIP_FIELDS and (info.repr or gen.override[cls.__name__])
                ] for cls in gen.order
            },
        }
        (UI_OUT / "upstream.json").write_text(
            json.dumps(snapshot, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
        )
        print(f"ok: {len(gen.order)} 个类，{version}，upstream {revision}")
    finally:
        os.chdir(REPO)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

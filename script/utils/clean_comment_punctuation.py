"""清理 Python 源码注释（以及可选的 docstring）中的中文标点.

用法：
    python script/utils/clean_comment_punctuation.py [PATH ...]
        [--include-docstrings] [--check] [--ext .py,.pyi]

默认只处理 ``# ...`` 形式的注释 token, 普通字符串字面量（包含 docstring）
保持不动；只有传入 ``--include-docstrings`` 时, 才会同时清理模块、类、
函数顶部的 docstring.

脚本基于 :mod:`tokenize` 解析源码, 因此普通字符串、f-string、字节串等
中的中文标点不会被误改.
"""

from __future__ import annotations

import argparse
import io
import sys
import token
import tokenize
from pathlib import Path
from typing import Iterable

# 中文（全角）标点到 ASCII 标点的映射表.
# 对于没有完全对应 ASCII 字符的标点, 映射为最贴近的替代字符或空格.
PUNCT_MAP: dict[str, str] = {
    "，": ", ",
    "。": ". ",
    "！": "! ",
    "？": "? ",
    "；": "; ",
    "：": ": ",
    "、": ", ",
    "（": " (",
    "）": ") ",
    "【": " [",
    "】": "] ",
    "「": " [",
    "」": "] ",
    "『": " [",
    "』": "] ",
    "《": " <",
    "》": "> ",
    "“": '"',
    "”": '"',
    "‘": "'",
    "’": "'",
    "～": "~",
    "·": "`",
    "—": "-",
    "–": "-",
    "…": "...",
    "·": ".",
    "％": "%",
    "＃": "#",
    "＆": "&",
    "＊": "*",
    "＠": "@",
    "／": "/",
    "＼": "\\",
    "｜": "|",
    "＋": "+",
    "＝": "=",
    "＜": "<",
    "＞": ">",
    "\u3000": " ",  # 全角空格
}


def replace_punct(text: str) -> str:
    out = []
    for ch in text:
        out.append(PUNCT_MAP.get(ch, ch))
    return "".join(out)


def _is_docstring(tokens: list[tokenize.TokenInfo], idx: int) -> bool:
    """判断位于 ``idx`` 的 STRING token 是否为 docstring.

    判定规则：该字符串必须是模块、类或函数体内的第一条语句, 即向前回溯
    时只会遇到 NEWLINE / INDENT / NL / ENCODING / COMMENT, 或一个表示
    ``def`` / ``class`` 头部结束的 ``:``.
    """
    # 向前回溯, 跳过对语义无影响的 token.
    i = idx - 1
    while i >= 0:
        tok = tokens[i]
        if tok.type in (token.NEWLINE, token.NL, token.INDENT, tokenize.ENCODING, tokenize.COMMENT):
            i -= 1
            continue
        # 模块级 docstring：之前没有任何实质性 token.
        if tok.type == token.ENCODING:
            return True
        # 紧跟在 def/class 头部的 ``:`` 之后, 视为函数/类 docstring.
        if tok.type == token.OP and tok.string == ":":
            return True
        return False
    # 已经回溯到文件起始, 视为模块级 docstring.
    return True


def clean_source(source: str, *, include_docstrings: bool = False) -> str:
    """返回去除注释中中文标点后的源码字符串.

    通过 tokenize 准确识别注释 token, 可选地处理 docstring, 
    其它字符串字面量保持不动.
    """
    try:
        tokens = list(tokenize.generate_tokens(io.StringIO(source).readline))
    except tokenize.TokenizeError:
        # 兜底：tokenize 失败时退化为基于行的 ``#`` 注释扫描.
        return _clean_comments_linewise(source)

    # 收集所有需要替换的片段：(起点, 终点, 新字符串).
    edits: list[tuple[tuple[int, int], tuple[int, int], str]] = []
    for i, tok in enumerate(tokens):
        if tok.type == tokenize.COMMENT:
            new = replace_punct(tok.string)
            if new != tok.string:
                edits.append((tok.start, tok.end, new))
        elif include_docstrings and tok.type == token.STRING:
            if _is_docstring(tokens, i):
                new = replace_punct(tok.string)
                if new != tok.string:
                    edits.append((tok.start, tok.end, new))

    if not edits:
        return source

    # 按行应用替换.注释一定是单行, docstring 可能跨行.
    lines = source.splitlines(keepends=True)
    # 按起始位置倒序应用, 避免前面的修改影响后续偏移.
    for (srow, scol), (erow, ecol), new in sorted(edits, key=lambda e: e[0], reverse=True):
        srow_i = srow - 1
        erow_i = erow - 1
        if srow_i == erow_i:
            line = lines[srow_i]
            lines[srow_i] = line[:scol] + new + line[ecol:]
        else:
            first = lines[srow_i][:scol]
            last = lines[erow_i][ecol:]
            # 拼回 token 前后的内容；末行 ``ecol`` 之后的部分原样保留.
            replacement = first + new + last
            # 替换内容自身可能包含换行, 重新切分回多行.
            new_lines = replacement.splitlines(keepends=True)
            lines[srow_i : erow_i + 1] = new_lines
    return "".join(lines)


def _clean_comments_linewise(source: str) -> str:
    """tokenize 失败时使用的兜底实现：在明显的字符串引号之外查找 ``#``.

    这是一个尽力而为的路径, 无法处理所有边界情况.
    """
    out_lines = []
    for line in source.splitlines(keepends=True):
        in_s: str | None = None
        i = 0
        n = len(line)
        comment_idx = -1
        while i < n:
            ch = line[i]
            if in_s:
                if ch == "\\" and i + 1 < n:
                    i += 2
                    continue
                if ch == in_s:
                    in_s = None
            elif ch in ("'", '"'):
                in_s = ch
            elif ch == "#":
                comment_idx = i
                break
            i += 1
        if comment_idx >= 0:
            out_lines.append(line[:comment_idx] + replace_punct(line[comment_idx:]))
        else:
            out_lines.append(line)
    return "".join(out_lines)


def iter_files(paths: Iterable[Path], exts: set[str]) -> Iterable[Path]:
    for p in paths:
        if p.is_file():
            if p.suffix in exts:
                yield p
        elif p.is_dir():
            for sub in p.rglob("*"):
                if sub.is_file() and sub.suffix in exts and not _is_ignored(sub):
                    yield sub


_IGNORED_DIRS = {".git", ".venv", "venv", "__pycache__", "build", "dist", "runtime", ".ruff_cache", "node_modules"}


def _is_ignored(p: Path) -> bool:
    return any(part in _IGNORED_DIRS for part in p.parts)


def process_file(path: Path, *, include_docstrings: bool, check: bool) -> bool:
    """返回文件是否被修改（在 --check 模式下表示是否会被修改）."""
    try:
        original = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return False
    cleaned = clean_source(original, include_docstrings=include_docstrings)
    if cleaned == original:
        return False
    if check:
        return True
    path.write_text(cleaned, encoding="utf-8")
    return True


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "paths",
        nargs="*",
        default=["src", "script", "main.py"],
        help="要扫描的文件或目录（默认：src script main.py）.",
    )
    parser.add_argument(
        "--include-docstrings",
        action="store_true",
        help="同时清理模块、类、函数 docstring 中的中文标点.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="只检查不修改；如果存在需要修改的文件, 退出码为 1.",
    )
    parser.add_argument(
        "--ext",
        default=".py",
        help="要处理的文件扩展名, 逗号分隔（默认：.py）.",
    )
    args = parser.parse_args(argv)

    exts = {e if e.startswith(".") else f".{e}" for e in args.ext.split(",") if e.strip()}
    targets = [Path(p) for p in args.paths]

    changed: list[Path] = []
    for f in iter_files(targets, exts):
        if process_file(f, include_docstrings=args.include_docstrings, check=args.check):
            changed.append(f)
            print(f"{'would change' if args.check else 'cleaned'}: {f}")

    print(f"\n{len(changed)} file(s) {'would be changed' if args.check else 'changed'}.")
    if args.check and changed:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

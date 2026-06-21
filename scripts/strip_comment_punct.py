#!/usr/bin/env python3
"""注释标点英文化：中文标点→英文，删中文句号。

只动注释（// 行注释、/* */ 块注释），不碰代码和字符串。
中文标点→英文：，→,  ：→:  ；→;  （→(  ）→)  "→"  '→'  ？→?  ！→!  、→,
中文句号 。→ 删（真人注释习惯不打句号）。
英文 . 保留（数字/路径/版本如 3.14、file.rs、1.0）。

用法: python scripts/strip_comment_punct.py <file.rs>...
"""
import sys
import pathlib

PUNCT_MAP = {
    '，': ',',
    '。': '',
    '：': ':',
    '；': ';',
    '（': '(',
    '）': ')',
    '"': '"',
    '"': '"',
    ''': "'",
    ''': "'",
    '？': '?',
    '！': '!',
    '、': ',',
}


def strip(text: str) -> str:
    out = []
    i = 0
    n = len(text)
    in_str = False
    in_block = False
    in_line = False
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ''
        if in_line:
            if c == '\n':
                in_line = False
                out.append(c)
            else:
                out.append(PUNCT_MAP.get(c, c))
            i += 1
        elif in_block:
            if c == '*' and nxt == '/':
                in_block = False
                out.append('*/')
                i += 2
            else:
                out.append(PUNCT_MAP.get(c, c))
                i += 1
        elif in_str:
            out.append(c)
            if c == '\\' and nxt:
                out.append(nxt)
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
        else:
            if c == '/' and nxt == '/':
                in_line = True
                out.append('//')
                i += 2
            elif c == '/' and nxt == '*':
                in_block = True
                out.append('/*')
                i += 2
            elif c == '"':
                in_str = True
                out.append(c)
                i += 1
            else:
                out.append(c)
                i += 1
    return ''.join(out)


def main():
    changed = 0
    for path in sys.argv[1:]:
        p = pathlib.Path(path)
        src = p.read_text(encoding='utf-8')
        dst = strip(src)
        if dst != src:
            p.write_text(dst, encoding='utf-8')
            print(f"cleaned: {path}")
            changed += 1
    print(f"total: {changed} file(s) changed")


if __name__ == '__main__':
    main()

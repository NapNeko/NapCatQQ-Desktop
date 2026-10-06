import { describe, expect, it } from 'vitest';
import { cutAnsi, parseAnsi, type AnsiText } from './ansi';

/** 每段样式对应的文字，断言时比偏移好读 */
function pieces(t: AnsiText) {
    return t.spans.map((s) => ({ text: t.text.slice(s.start, s.end), style: s.style }));
}

describe('parseAnsi', () => {
    it('麦麦一行：时间戳按等级上色，模块名和正文用模块色', () => {
        const raw =
            '\x1b[38;5;117m09-26 16:29:00\x1b[0m \x1b[1;38;2;255;255;255m[主程序]\x1b[0m ' +
            '\x1b[1;38;2;255;255;255m正在启动MaiBot\x1b[0m';
        const parsed = parseAnsi(raw);
        expect(parsed.text).toBe('09-26 16:29:00 [主程序] 正在启动MaiBot');
        const white = { kind: 'rgb', r: 255, g: 255, b: 255 };
        expect(pieces(parsed)).toEqual([
            { text: '09-26 16:29:00', style: { fg: { kind: 'palette', index: 117 } } },
            { text: '[主程序]', style: { bold: true, fg: white } },
            { text: '正在启动MaiBot', style: { bold: true, fg: white } },
        ]);
    });

    it('NoneBot 的 loguru：标准色、粗体、下划线，39 只清前景色', () => {
        const raw =
            '\x1b[32m09-26 11:15:42\x1b[0m [\x1b[32m\x1b[1mSUCCESS\x1b[0m] ' +
            '\x1b[36m\x1b[4mnonebot\x1b[0m\x1b[36m\x1b[0m | NoneBot is initializing...';
        const parsed = parseAnsi(raw);
        expect(parsed.text).toBe('09-26 11:15:42 [SUCCESS] nonebot | NoneBot is initializing...');
        expect(pieces(parsed)).toEqual([
            { text: '09-26 11:15:42', style: { fg: { kind: 'palette', index: 2 } } },
            { text: 'SUCCESS', style: { fg: { kind: 'palette', index: 2 }, bold: true } },
            { text: 'nonebot', style: { fg: { kind: 'palette', index: 6 }, underline: true } },
        ]);
        expect(pieces(parseAnsi('\x1b[1;31mA\x1b[39mB\x1b[22mC'))).toEqual([
            { text: 'A', style: { bold: true, fg: { kind: 'palette', index: 1 } } },
            { text: 'B', style: { bold: true } },
        ]);
    });

    it('亮色 90–97、256 色、真彩和冒号写法', () => {
        expect(pieces(parseAnsi('\x1b[90m[Karin]\x1b[39m x'))[0].style.fg).toEqual({
            kind: 'palette',
            index: 8,
        });
        expect(pieces(parseAnsi('\x1b[38;5;208mdebug'))[0].style.fg).toEqual({
            kind: 'palette',
            index: 208,
        });
        expect(pieces(parseAnsi('\x1b[38;2;1;2;3mx'))[0].style.fg).toEqual({
            kind: 'rgb',
            r: 1,
            g: 2,
            b: 3,
        });
        expect(pieces(parseAnsi('\x1b[38:2::10:20:30mx'))[0].style.fg).toEqual({
            kind: 'rgb',
            r: 10,
            g: 20,
            b: 30,
        });
        expect(pieces(parseAnsi('\x1b[38:5:117mx'))[0].style.fg).toEqual({
            kind: 'palette',
            index: 117,
        });
    });

    it('背景色不画，但它的参数要跳过，后面的前景色照常认', () => {
        expect(pieces(parseAnsi('\x1b[48;5;21;31mX'))).toEqual([
            { text: 'X', style: { fg: { kind: 'palette', index: 1 } } },
        ]);
        expect(pieces(parseAnsi('\x1b[48;2;9;9;9;33mY'))).toEqual([
            { text: 'Y', style: { fg: { kind: 'palette', index: 3 } } },
        ]);
        expect(parseAnsi('\x1b[44mplain').spans).toEqual([]);
    });

    it('别的控制序列和控制符丢掉，只留文字', () => {
        expect(parseAnsi('\x1b[2K\x1b[1Gdone').text).toBe('done');
        expect(parseAnsi('\x1b]0;title\x07hello').text).toBe('hello');
        expect(parseAnsi('a\x07b\x00c\td').text).toBe('abc\td');
        expect(parseAnsi('\x1b[?25lx\x1b[?25h').text).toBe('x');
    });

    it('回车照终端的意思：后面的盖掉前面的，最后是空段就留前一段', () => {
        expect(parseAnsi(' 10%|#   |\r 50%|##  |\r100%|####|').text).toBe('100%|####|');
        expect(parseAnsi('done\r').text).toBe('done');
        const styled = parseAnsi('\x1b[32mold\rnew');
        expect(pieces(styled)).toEqual([
            { text: 'new', style: { fg: { kind: 'palette', index: 2 } } },
        ]);
    });
});

describe('cutAnsi', () => {
    it('挖掉时间前缀，后面的样式段往前挪', () => {
        const parsed = parseAnsi(
            '\x1b[33m09-26 16:29:02\x1b[0m \x1b[38;2;162;255;0m[配置]\x1b[0m 缺配置',
        );
        const body = cutAnsi(parsed, [[0, 15]]);
        expect(body.text).toBe('[配置] 缺配置');
        expect(pieces(body)).toEqual([
            { text: '[配置]', style: { fg: { kind: 'rgb', r: 162, g: 255, b: 0 } } },
        ]);
    });

    it('中间挖掉一段（AstrBot 的等级标签），两边的段各自对齐', () => {
        const parsed = parseAnsi('[Core] \x1b[1m[INFO]\x1b[0m [mod:1]: \x1b[1mhello\x1b[0m');
        const body = cutAnsi(parsed, [[7, 14]]);
        expect(body.text).toBe('[Core] [mod:1]: hello');
        expect(pieces(body)).toEqual([{ text: 'hello', style: { bold: true } }]);
    });

    it('区间乱序、重叠、越界都能处理；没东西可挖时原样返回', () => {
        const parsed = parseAnsi('abcdef');
        expect(
            cutAnsi(parsed, [
                [4, 99],
                [0, 2],
                [1, 3],
            ]).text,
        ).toBe('d');
        expect(cutAnsi(parsed, [])).toBe(parsed);
    });
});

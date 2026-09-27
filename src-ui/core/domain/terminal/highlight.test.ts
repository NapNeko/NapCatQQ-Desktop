import { describe, expect, it } from 'vitest';
import { applySgr, colorizeText, createKeywordHighlighter } from './highlight';

const E = '\x1b';

describe('colorizeText', () => {
    it('wraps keywords and resets right after', () => {
        expect(colorizeText('build failed with 2 errors')).toBe(
            `build ${E}[31mfailed${E}[0m with 2 ${E}[31merrors${E}[0m`,
        );
        expect(colorizeText('WARNING: disk almost full')).toBe(`${E}[33mWARNING${E}[0m: disk almost full`);
        expect(colorizeText('安装成功')).toBe(`安装${E}[32m成功${E}[0m`);
        expect(colorizeText('listen 192.168.1.10:8080 now')).toBe(
            `listen ${E}[35m192.168.1.10:8080${E}[0m now`,
        );
        expect(colorizeText('see https://example.com/a.')).toBe(
            `see ${E}[36mhttps://example.com/a${E}[0m.`,
        );
        expect(colorizeText('[INFO] start [DEBUG] x')).toBe(
            `[${E}[34mINFO${E}[0m] start [${E}[90mDEBUG${E}[0m] x`,
        );
    });

    it('leaves words that only contain a keyword alone', () => {
        expect(colorizeText('stderr terror failover okay')).toBe('stderr terror failover okay');
        expect(colorizeText('999.1.1.1')).toBe('999.1.1.1');
    });
});

describe('applySgr', () => {
    it('tracks when the style is back to default', () => {
        expect(applySgr(0, '31')).not.toBe(0);
        expect(applySgr(applySgr(0, '31'), '39')).toBe(0);
        expect(applySgr(applySgr(0, '1;31'), '22;39')).toBe(0);
        expect(applySgr(applySgr(0, '38;2;255;0;0'), '0')).toBe(0);
        expect(applySgr(0, '38;5;196')).not.toBe(0);
        expect(applySgr(applySgr(0, '4'), '')).toBe(0);
    });
});

describe('createKeywordHighlighter', () => {
    it('does not touch text the program already colored', () => {
        const h = createKeywordHighlighter();
        const input = `${E}[31merror${E}[0m and error`;
        expect(h.process(input)).toBe(`${E}[31merror${E}[0m and ${E}[31merror${E}[0m`);
    });

    it('keeps escape sequences split across chunks intact', () => {
        const h = createKeywordHighlighter();
        const a = h.process(`${E}[3`);
        const b = h.process(`1mred error${E}[0m done`);
        expect(a + b).toBe(`${E}[31mred error${E}[0m ${E}[32mdone${E}[0m`);
    });

    it('skips the alternate screen', () => {
        const h = createKeywordHighlighter();
        const out = h.process(`${E}[?1049herror${E}[?1049l error`);
        expect(out).toBe(`${E}[?1049herror${E}[?1049l ${E}[31merror${E}[0m`);
    });

    it('skips the prompt and the typed line, then colors the output', () => {
        const h = createKeywordHighlighter();
        const prompt = `${E}]633;A\x07u@h:~/error$ ${E}]633;B\x07`;
        expect(h.process(prompt)).toBe(prompt);
        expect(h.process('grep error')).toBe('grep error');
        expect(h.process('\r\nfatal error\r\n')).toBe(
            `\r\n${E}[31mfatal${E}[0m ${E}[31merror${E}[0m\r\n`,
        );
    });

    it('treats an ESC that cuts an OSC short as a new sequence', () => {
        const h = createKeywordHighlighter();
        const out = h.process(`${E}]0;title${E}[31merror${E}[0m`);
        expect(out).toBe(`${E}]0;title${E}[31merror${E}[0m`);
    });

    it('a keyword split between chunks is left plain', () => {
        const h = createKeywordHighlighter();
        expect(h.process('er') + h.process('ror')).toBe('error');
    });
});

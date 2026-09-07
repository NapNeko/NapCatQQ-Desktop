import { describe, expect, it } from 'vitest';
import { appendLine, canonicalizeLogEntry, stripAnsiEscapes } from './log-buffer';
import type { LogEntry } from './log-buffer';

describe('stripAnsiEscapes', () => {
    it('剥掉 Karin chalk 的 CSI 颜色码', () => {
        const raw = '\x1b[90m[Karin][20:16:22.716][MARK]\x1b[39m Karin 启动中...';
        expect(stripAnsiEscapes(raw)).toBe('[Karin][20:16:22.716][MARK] Karin 启动中...');
    });
});

describe('appendLine · Karin', () => {
    it('清洗 ANSI 后按主题等级入库，正文不再带 [Karin][时:分:秒][LEVEL]', () => {
        const raw = '\x1b[90m[Karin][20:16:22.716][MARK]\x1b[39m Karin 启动中...';
        const [entry] = appendLine([], raw, 'stdout', '00:00:00');
        expect(entry.text).toBe('Karin 启动中...');
        expect(entry.level).toBe('trace');
        expect(entry.timestamp).toBe('20:16:22');
        expect(entry.text).not.toMatch(/\u001b/);
    });

    it('INFO 行走 info，保留 [server] 这类正文前缀', () => {
        const raw =
            '\x1b[32m[Karin][20:16:22.739][INFO]\x1b[39m [server] express 正在监听: http://127.0.0.1:7777';
        const [entry] = appendLine([], raw, 'stdout', '00:00:00');
        expect(entry.level).toBe('info');
        expect(entry.text).toBe('[server] express 正在监听: http://127.0.0.1:7777');
        expect(entry.timestamp).toBe('20:16:22');
    });
});

describe('appendLine · NapCat', () => {
    it('无 ANSI 的 NC 行仍按原规则拆时间和等级', () => {
        const [entry] = appendLine([], '07-11 17:06:19 [info] nick | hello', 'stdout', '00:00:00');
        expect(entry.level).toBe('info');
        expect(entry.timestamp).toBe('17:06:19');
        expect(entry.text).toBe('nick | hello');
    });
});

describe('canonicalizeLogEntry', () => {
    it('把已经进缓冲的脏行就地洗成主题行', () => {
        const dirty: LogEntry = {
            id: 'old',
            text: '\x1b[90m[Karin][20:16:22.716][MARK]\x1b[39m Karin 启动中...',
            channel: 'stdout',
            level: 'unknown',
            timestamp: '20:16:22',
        };
        const next = canonicalizeLogEntry(dirty);
        expect(next.text).toBe('Karin 启动中...');
        expect(next.level).toBe('trace');
        expect(next.id).toBe('old');
    });
});

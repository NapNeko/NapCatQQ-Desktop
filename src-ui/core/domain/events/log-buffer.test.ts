import { describe, expect, it } from 'vitest';
import {
    appendLine,
    buildHistoryEntries,
    canonicalizeLogEntry,
    stripAnsiEscapes,
} from './log-buffer';
import type { LogEntry } from './log-buffer';

function one(raw: string): LogEntry {
    const [entry] = appendLine([], raw, 'stdout', '00:00:00');
    return entry;
}

function spanTexts(entry: LogEntry): string[] {
    return (entry.spans ?? []).map((s) => entry.text.slice(s.start, s.end));
}

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
        // eslint-disable-next-line no-control-regex -- 断言清洗后不残留 ESC
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

describe('appendLine · Koishi', () => {
    it('全年月日 + 单字母等级 + 来源名：时间进列、等级入色、来源名挖出来', () => {
        const entry = one('2026-09-29 21:22:14 [I] loader apply plugin help:j48bsq');
        expect(entry.level).toBe('info');
        expect(entry.timestamp).toBe('21:22:14');
        expect(entry.scope).toBe('loader');
        expect(entry.text).toBe('apply plugin help:j48bsq');
    });

    it('W/E/S/D 各字母对到 warn/error/success/debug', () => {
        expect(one('2026-09-29 21:22:15 [W] config something').level).toBe('warn');
        expect(one('2026-09-29 21:22:15 [E] adapter-onebot Error: boom').level).toBe('error');
        expect(one('2026-09-29 21:22:15 [S] telemetry ').level).toBe('success');
        expect(one('2026-09-29 21:22:15 [D] sqlite query').level).toBe('debug');
    });

    it('消息为空的行只剩来源名；堆栈行算续行跟着上一条的等级', () => {
        const first = one('2026-09-29 21:22:15 [S] telemetry ');
        expect(first.scope).toBe('telemetry');
        expect(first.text).toBe('');
        let logs = appendLine([], '2026-09-29 21:22:15 [E] app Error: boom', 'stdout', '00:00:00');
        logs = appendLine(
            logs,
            '    at Object.<anonymous> (/app/index.js:1:1)',
            'stdout',
            '00:00:00',
        );
        expect(logs[1].level).toBe('error');
        expect(logs[1].continuation).toBe(true);
    });
});

describe('appendLine · 麦麦', () => {
    const mai = (color: string, body: string) =>
        `\x1b[${color}m09-26 16:29:02\x1b[0m \x1b[38;2;162;255;0m[配置]\x1b[0m \x1b[38;2;162;255;0m${body}\x1b[0m`;

    it('默认 lite 样式不写等级，按时间戳的颜色认', () => {
        expect(one(mai('38;5;117', '配置文件已加载')).level).toBe('info');
        expect(one(mai('33', '配置文件缺失')).level).toBe('warn');
        expect(one(mai('31', '编码失败')).level).toBe('error');
        expect(one(mai('38;5;208', '调试')).level).toBe('debug');
        expect(one(mai('35', '崩了')).level).toBe('fatal');
    });

    it('时间进时间列，正文保留模块色；正文里的单词不抢时间戳颜色', () => {
        const entry = one(mai('33', '控制台=INFO，文件=DEBUG'));
        expect(entry.level).toBe('warn');
        expect(entry.timestamp).toBe('16:29:02');
        expect(entry.text).toBe('[配置] 控制台=INFO，文件=DEBUG');
        expect(spanTexts(entry)).toEqual(['[配置]', '控制台=INFO，文件=DEBUG']);
    });

    it('full 样式补了空格的等级标签也认，并从正文拿掉', () => {
        const entry = one('09-26 16:29:00 [    INFO] [主程序] 启动');
        expect(entry.level).toBe('info');
        expect(entry.text).toBe('[主程序] 启动');
        expect(one('09-26 16:29:00 [CRITICAL] [主程序] 崩了').level).toBe('fatal');
    });
});

describe('appendLine · AstrBot', () => {
    it('方括号时间进时间列，[Core] 后面的等级标签从正文拿掉，粗体留着', () => {
        const entry = one(
            '\x1b[32m[18:55:54.222]\x1b[0m [Core] \x1b[1m[INFO]\x1b[0m [config.astrbot_config:199]: ' +
                '\x1b[1mConfig key missing; added default.\x1b[0m',
        );
        expect(entry.timestamp).toBe('18:55:54');
        expect(entry.level).toBe('info');
        expect(entry.text).toBe(
            '[Core] [config.astrbot_config:199]: Config key missing; added default.',
        );
        expect(spanTexts(entry)).toEqual(['Config key missing; added default.']);
    });

    it('插件行、它自带的 hypercorn 行', () => {
        const plugin = one(
            '[10:42:49.906] [astrbot_plugin_vikunja] [WARN] [v4.28.0] [astrbot-plugin-vikunja.main:228]: 未配置',
        );
        expect(plugin.level).toBe('warn');
        expect(plugin.text).toBe(
            '[astrbot_plugin_vikunja] [v4.28.0] [astrbot-plugin-vikunja.main:228]: 未配置',
        );
        const hypercorn = one(
            '[2026-09-27 10:42:49 +0800] [20504] [INFO] Running on http://0.0.0.0:6185',
        );
        expect(hypercorn.timestamp).toBe('10:42:49');
        expect(hypercorn.level).toBe('info');
        expect(hypercorn.text).toBe('[20504] Running on http://0.0.0.0:6185');
    });
});

describe('appendLine · NoneBot2', () => {
    it('带颜色的 loguru 行：等级进列，模块名留着颜色', () => {
        const entry = one(
            '\x1b[32m09-26 11:15:42\x1b[0m [\x1b[32m\x1b[1mSUCCESS\x1b[0m] ' +
                '\x1b[36m\x1b[4mnonebot\x1b[0m\x1b[36m\x1b[0m | NoneBot is initializing...',
        );
        expect(entry.level).toBe('success');
        expect(entry.timestamp).toBe('11:15:42');
        expect(entry.text).toBe('nonebot | NoneBot is initializing...');
        expect(spanTexts(entry)).toEqual(['nonebot']);
    });
});

describe('appendLine · NeoBot', () => {
    // loguru 控制台格式：{time} | {level: <8} | {name: <24} | {message}，colorize=True
    it('裸 ISO 时间与竖线等级：时间进列、等级入色、正文只剩模块与消息', () => {
        const entry = one(
            '\x1b[32m2026-10-04 03:41:22.123\x1b[0m | \x1b[1m\x1b[34mINFO    \x1b[0m | ' +
                '\x1b[36mneobot.chat             \x1b[0m | \x1b[1m\x1b[34m收到群消息\x1b[0m',
        );
        expect(entry.level).toBe('info');
        expect(entry.timestamp).toBe('03:41:22');
        expect(entry.text).toBe('neobot.chat              | 收到群消息');
    });

    it('WARNING 的 loguru 名也要归到 warn，不是 unknown', () => {
        const entry = one('2026-10-04 03:41:23.000 | WARNING  | neobot.config | 缺 API key');
        expect(entry.level).toBe('warn');
        expect(entry.timestamp).toBe('03:41:23');
        expect(entry.text).toBe('neobot.config | 缺 API key');
    });

    it('无颜色的裸行同样能拆（重定向到文件后 ANSI 会被剥掉）', () => {
        const entry = one('2026-10-04 03:41:24.500 | ERROR    | neobot.llm | 上游超时');
        expect(entry.level).toBe('error');
        expect(entry.timestamp).toBe('03:41:24');
        expect(entry.text).toBe('neobot.llm | 上游超时');
    });

    it('CRITICAL 归到 fatal、SUCCESS 归到 success，等级标签同样从正文挖掉', () => {
        const critical = one('2026-10-04 03:41:25.000 | CRITICAL | neobot.core | 启动失败');
        expect(critical.level).toBe('fatal');
        expect(critical.text).toBe('neobot.core | 启动失败');
        const success = one('2026-10-04 03:41:26.000 | SUCCESS  | neobot.core | 初始化完成');
        expect(success.level).toBe('success');
        expect(success.text).toBe('neobot.core | 初始化完成');
    });
});

describe('续行', () => {
    it('没时间没等级的行接着上一条：等级跟着走，缩进留着', () => {
        const logs = buildHistoryEntries(
            [
                '\x1b[31m09-27 11:14:47\x1b[0m [记忆嵌入] 编码失败',
                'Traceback (most recent call last):',
                '  File "bot.py", line 25, in <module>',
                '09-27 11:14:48 [INFO] nonebot | 恢复',
            ],
            '00:00:00',
        );
        expect(logs.map((l) => [l.level, !!l.continuation])).toEqual([
            ['error', false],
            ['error', true],
            ['error', true],
            ['info', false],
        ]);
        expect(logs[2].text).toBe('  File "bot.py", line 25, in <module>');
    });

    it('第一行前面没东西可接；自己写了等级的不算续行', () => {
        expect(one('Welcome to AstrBot CLI!').continuation).toBeUndefined();
        const [, second] = buildHistoryEntries(
            ['09-27 11:14:47 [INFO] a | b', '[Karin][INFO] heartbeat #1'],
            '00:00:00',
        );
        expect(second.continuation).toBeUndefined();
        expect(second.level).toBe('info');
        expect(second.text).toBe('[Karin] heartbeat #1');
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

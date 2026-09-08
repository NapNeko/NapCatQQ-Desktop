import { describe, expect, it } from 'vitest';
import { SEE_LOGS_HINT, briefError, errorBarContent } from './errorBarCopy';

describe('briefError', () => {
    it('picks the last Python error line', () => {
        expect(
            briefError(
                'Traceback (most recent call last):\n  File "bot.py", line 1\nRuntimeError: driver does not support http client',
            ),
        ).toBe('RuntimeError: driver does not support http client');
    });

    it('strips app-framework prefixes', () => {
        expect(briefError('应用端运行失败: 依赖无法解析')).toBe('依赖无法解析');
    });
});

describe('errorBarContent', () => {
    it('appends the log hint', () => {
        expect(errorBarContent('启动失败')).toBe(`启动失败。${SEE_LOGS_HINT}`);
    });

    it('falls back to the hint when empty', () => {
        expect(errorBarContent('')).toBe(SEE_LOGS_HINT);
    });
});

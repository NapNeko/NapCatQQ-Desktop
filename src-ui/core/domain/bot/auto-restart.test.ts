import { describe, expect, it } from 'vitest';

import type { AutoRestartSchedule } from '../../ipc/generated/domain/AutoRestartSchedule';
import {
    cronFromRecipe,
    describeAutoRestartSchedule,
    describeCronRecipe,
    formatCronPreview,
    recipeFromCron,
    type CronRecipe,
} from './auto-restart';

const base: AutoRestartSchedule = {
    enable: true,
    mode: 'interval',
    time_unit: 'h',
    duration: 6,
    cron: '',
};

describe('describeAutoRestartSchedule', () => {
    it('未启用返回 null', () => {
        expect(describeAutoRestartSchedule({ ...base, enable: false })).toBeNull();
    });

    it('间隔模式按单位描述', () => {
        expect(describeAutoRestartSchedule(base)).toBe('每 6小时');
        expect(describeAutoRestartSchedule({ ...base, time_unit: 'd', duration: 1 })).toBe('每 1天');
    });

    it('cron 模式能识别的形状说人话，否则回落到表达式', () => {
        expect(describeAutoRestartSchedule({ ...base, mode: 'cron', cron: ' 0 4 * * * ' })).toBe(
            '每天 04:00',
        );
        expect(describeAutoRestartSchedule({ ...base, mode: 'cron', cron: '*/30 * * * *' })).toBe(
            'cron */30 * * * *',
        );
        expect(describeAutoRestartSchedule({ ...base, mode: 'cron', cron: '' })).toBe('cron 未填写');
    });
});

describe('recipeFromCron / cronFromRecipe', () => {
    const cases: Array<[string, CronRecipe]> = [
        ['0 4 * * *', { kind: 'daily', hour: 4, minute: 0 }],
        ['30 23 * * *', { kind: 'daily', hour: 23, minute: 30 }],
        ['0 4 * * 1', { kind: 'weekly', weekday: 1, hour: 4, minute: 0 }],
        ['0 4 1 * *', { kind: 'monthly', day: 1, hour: 4, minute: 0 }],
        ['5 6 31 * *', { kind: 'monthly', day: 31, hour: 6, minute: 5 }],
    ];

    it.each(cases)('%s 往返一致', (expr, recipe) => {
        expect(recipeFromCron(expr)).toEqual(recipe);
        expect(cronFromRecipe(recipe)).toBe(expr);
    });

    it('周字段接受 7 与英文名，并归一到 0-6', () => {
        expect(recipeFromCron('0 4 * * 7')).toEqual({ kind: 'weekly', weekday: 0, hour: 4, minute: 0 });
        expect(recipeFromCron('0 4 * * fri')).toEqual({
            kind: 'weekly',
            weekday: 5,
            hour: 4,
            minute: 0,
        });
    });

    it('认不出的形状原样当 custom', () => {
        for (const expr of [
            '*/30 * * * *',
            '0 4 * * 1-5',
            '0 4 1 6 *',
            '0 4 1 * 1',
            '0 0 4 * * *',
            '0 24 * * *',
            '0 4 0 * *',
            '',
            '  garbage ',
        ]) {
            expect(recipeFromCron(expr)).toEqual({ kind: 'custom', expr });
        }
    });
});

describe('describeCronRecipe', () => {
    it('三种结构化形状 + custom', () => {
        expect(describeCronRecipe({ kind: 'daily', hour: 4, minute: 5 })).toBe('每天 04:05');
        expect(describeCronRecipe({ kind: 'weekly', weekday: 0, hour: 12, minute: 0 })).toBe(
            '每周日 12:00',
        );
        expect(describeCronRecipe({ kind: 'monthly', day: 15, hour: 0, minute: 30 })).toBe(
            '每月 15 日 00:30',
        );
        expect(describeCronRecipe({ kind: 'custom', expr: '*/5 * * * *' })).toBe('cron */5 * * * *');
    });
});

describe('formatCronPreview', () => {
    it('空列表 / 全部无效返回 null', () => {
        expect(formatCronPreview([])).toBeNull();
        expect(formatCronPreview(['not-a-date'])).toBeNull();
    });

    it('按本地时间格式化并用 · 连接', () => {
        const a = new Date(2026, 8, 7, 4, 0).toISOString();
        const b = new Date(2026, 8, 8, 4, 0).toISOString();
        expect(formatCronPreview([a, 'bad', b])).toBe('09-07 04:00 · 09-08 04:00');
    });
});

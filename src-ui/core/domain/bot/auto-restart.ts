// 定时重启计划的展示文案，纯函数。

import type { AutoRestartSchedule } from '../../ipc/generated/domain/AutoRestartSchedule';
import type { TimeUnit } from '../../ipc/generated/domain/TimeUnit';

export function formatTimeUnit(unit: TimeUnit): string {
    switch (unit) {
        case 'm':
            return '分钟';
        case 'h':
            return '小时';
        case 'd':
            return '天';
        case 'mon':
            return '个月';
        case 'year':
            return '年';
        default:
            return String(unit);
    }
}

/**
 * 「固定时间点」的结构化表示：新手只碰 daily / weekly / monthly，
 * 表达式落盘仍是 cron；解析不出这三种形状的一律当 custom 原样保留。
 * weekday 0 = 周日 … 6 = 周六（cron 口径）。
 */
export type CronRecipe =
    | { kind: 'daily'; hour: number; minute: number }
    | { kind: 'weekly'; weekday: number; hour: number; minute: number }
    | { kind: 'monthly'; day: number; hour: number; minute: number }
    | { kind: 'custom'; expr: string };

export type CronRecipeKind = CronRecipe['kind'];

/** 切到「固定时间点」时的起手值：每天 04:00（低峰）。 */
export const DEFAULT_CRON_RECIPE = { kind: 'daily', hour: 4, minute: 0 } as const satisfies CronRecipe;

export function cronFromRecipe(recipe: CronRecipe): string {
    switch (recipe.kind) {
        case 'daily':
            return `${recipe.minute} ${recipe.hour} * * *`;
        case 'weekly':
            return `${recipe.minute} ${recipe.hour} * * ${recipe.weekday}`;
        case 'monthly':
            return `${recipe.minute} ${recipe.hour} ${recipe.day} * *`;
        case 'custom':
            return recipe.expr;
    }
}

const WEEKDAY_NAMES = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT'];

function parseIntInRange(field: string, min: number, max: number): number | null {
    if (!/^\d{1,2}$/.test(field)) return null;
    const n = Number(field);
    return n >= min && n <= max ? n : null;
}

function parseWeekday(field: string): number | null {
    const numeric = parseIntInRange(field, 0, 7);
    if (numeric != null) return numeric === 7 ? 0 : numeric;
    const idx = WEEKDAY_NAMES.indexOf(field.toUpperCase());
    return idx >= 0 ? idx : null;
}

export function recipeFromCron(expr: string): CronRecipe {
    const custom: CronRecipe = { kind: 'custom', expr };
    const fields = expr.trim().split(/\s+/);
    if (fields.length !== 5) return custom;
    const [minuteField, hourField, dom, month, dow] = fields;
    const minute = parseIntInRange(minuteField, 0, 59);
    const hour = parseIntInRange(hourField, 0, 23);
    if (minute == null || hour == null || month !== '*') return custom;
    if (dom === '*' && dow === '*') return { kind: 'daily', hour, minute };
    if (dom === '*') {
        const weekday = parseWeekday(dow);
        return weekday == null ? custom : { kind: 'weekly', weekday, hour, minute };
    }
    if (dow === '*') {
        const day = parseIntInRange(dom, 1, 31);
        return day == null ? custom : { kind: 'monthly', day, hour, minute };
    }
    return custom;
}

export const WEEKDAY_LABELS = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

function pad2(n: number): string {
    return String(n).padStart(2, '0');
}

export function formatClock(hour: number, minute: number): string {
    return `${pad2(hour)}:${pad2(minute)}`;
}

/** 人话描述；custom 回落到 `cron <expr>`。 */
export function describeCronRecipe(recipe: CronRecipe): string {
    switch (recipe.kind) {
        case 'daily':
            return `每天 ${formatClock(recipe.hour, recipe.minute)}`;
        case 'weekly':
            return `每${WEEKDAY_LABELS[recipe.weekday] ?? '周'} ${formatClock(recipe.hour, recipe.minute)}`;
        case 'monthly':
            return `每月 ${recipe.day} 日 ${formatClock(recipe.hour, recipe.minute)}`;
        case 'custom': {
            const expr = recipe.expr.trim();
            return expr ? `cron ${expr}` : 'cron 未填写';
        }
    }
}

/** 卡片 / 列表用的一句话计划描述；未启用返回 null。 */
export function describeAutoRestartSchedule(sched: AutoRestartSchedule): string | null {
    if (!sched.enable) return null;
    if (sched.mode === 'cron') {
        return describeCronRecipe(recipeFromCron(sched.cron));
    }
    return `每 ${sched.duration}${formatTimeUnit(sched.time_unit)}`;
}

const previewFormatter = new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
});

/** 后端返回的 RFC3339 触发时刻 → "09-07 04:00 · 09-08 04:00"；解析失败的项跳过。 */
export function formatCronPreview(isoTimes: readonly string[]): string | null {
    const parts: string[] = [];
    for (const iso of isoTimes) {
        const ts = Date.parse(iso);
        if (Number.isNaN(ts)) continue;
        // zh-CN 输出形如 "09/07 04:00"，统一成短横线
        parts.push(previewFormatter.format(new Date(ts)).replace(/\//g, '-'));
    }
    return parts.length > 0 ? parts.join(' · ') : null;
}

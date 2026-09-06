// 定时自动重启的计划编辑器：一行读成句子——「每隔 6 小时」「每天 04:20」「每周 周一 04:20」。
// 第一个下拉同时决定 mode（每隔 = interval，其余 = cron）；cron 只在选「自定义」时露出原文。

import { useMemo, useState, type ReactNode } from 'react';
import {
    DayOfMonthPicker,
    NumberField,
    Select,
    TextField,
    TimePicker,
} from '../../../../shared/ui';
import {
    DEFAULT_CRON_RECIPE,
    WEEKDAY_LABELS,
    cronFromRecipe,
    recipeFromCron,
    type CronRecipe,
    type CronRecipeKind,
} from '../../../../core/domain/bot/auto-restart';
import { useCronPreview } from '../../../../hooks/bot/useCronPreview';
import type { AutoRestartSchedule } from '../../../../core/ipc/generated/domain/AutoRestartSchedule';
import type { TimeUnit } from '../../../../core/ipc/generated/domain/TimeUnit';

type Shape = 'interval' | CronRecipeKind;

const SHAPE_ITEMS: ReadonlyArray<{ value: Shape; label: string }> = [
    { value: 'interval', label: '每隔' },
    { value: 'daily', label: '每天' },
    { value: 'weekly', label: '每周' },
    { value: 'monthly', label: '每月' },
    { value: 'custom', label: '自定义 cron' },
];

const TIME_UNIT_ITEMS: ReadonlyArray<{ value: TimeUnit; label: string }> = [
    { value: 'm', label: '分钟' },
    { value: 'h', label: '小时' },
    { value: 'd', label: '天' },
    { value: 'mon', label: '月' },
    { value: 'year', label: '年' },
];

// 周一起排，值仍是 cron 口径（0 = 周日）
const WEEKDAY_ITEMS = [1, 2, 3, 4, 5, 6, 0].map((d) => ({
    value: String(d),
    label: WEEKDAY_LABELS[d],
}));

// 按钮型触发器（时刻 / 日期）对齐到 Select / 输入框的高度与字号
const PICKER_TRIGGER_CLASS = 'h-auto px-3 py-2 text-sm';

interface AutoRestartScheduleEditorProps {
    schedule: AutoRestartSchedule;
    onChange: (patch: Partial<AutoRestartSchedule>) => void;
}

export function AutoRestartScheduleEditor({ schedule, onChange }: AutoRestartScheduleEditorProps) {
    // 用户显式选「自定义」后锁定文本框视图，否则由表达式形状决定显示哪组控件
    const [forceCustom, setForceCustom] = useState(false);
    const parsed = useMemo(() => recipeFromCron(schedule.cron), [schedule.cron]);
    const recipe: CronRecipe = forceCustom ? { kind: 'custom', expr: schedule.cron } : parsed;
    const isCron = schedule.mode === 'cron';
    const shape: Shape = isCron ? recipe.kind : 'interval';
    const structured = isCron && recipe.kind !== 'custom' ? recipe : null;
    const preview = useCronPreview(isCron ? schedule.cron : '');

    const setShape = (next: Shape) => {
        if (next === 'interval') {
            onChange({ mode: 'interval' });
            return;
        }
        if (next === 'custom') {
            setForceCustom(true);
            onChange({ mode: 'cron' });
            return;
        }
        setForceCustom(false);
        // 换频率沿用表达式里能读出来的时刻 / 星期 / 日期；读不出来落到默认 04:00
        const from = parsed.kind === 'custom' ? DEFAULT_CRON_RECIPE : parsed;
        const clock = { hour: from.hour, minute: from.minute };
        const nextRecipe: CronRecipe =
            next === 'daily'
                ? { kind: next, ...clock }
                : next === 'weekly'
                    ? { kind: next, weekday: from.kind === 'weekly' ? from.weekday : 1, ...clock }
                    : { kind: next, day: from.kind === 'monthly' ? from.day : 1, ...clock };
        onChange({ mode: 'cron', cron: cronFromRecipe(nextRecipe) });
    };

    const setRecipe = (next: CronRecipe) => onChange({ cron: cronFromRecipe(next) });

    const status: ReactNode = (() => {
        if (!isCron) return null;
        if (preview.status === 'ok' && preview.text) return `下次 ${preview.text}`;
        if (recipe.kind === 'custom' && preview.status !== 'error') return '分 时 日 月 周 · 本地时区';
        if (structured?.kind === 'monthly' && structured.day > 28) return '短月份没有这一天时跳过';
        return null;
    })();

    return (
        <div className="flex flex-col gap-1.5">
            <div className="flex flex-wrap items-start gap-2">
                <Select className="w-32" items={SHAPE_ITEMS} value={shape} onValueChange={setShape} />

                {shape === 'interval' && (
                    <>
                        <NumberField
                            className="w-20"
                            value={schedule.duration}
                            min={1}
                            onValueChange={(v) => onChange({ duration: v ?? 0 })}
                        />
                        <Select
                            className="w-24"
                            items={TIME_UNIT_ITEMS}
                            value={schedule.time_unit}
                            onValueChange={(v) => onChange({ time_unit: v })}
                        />
                    </>
                )}

                {structured?.kind === 'weekly' && (
                    <Select
                        className="w-24"
                        items={WEEKDAY_ITEMS}
                        value={String(structured.weekday)}
                        onValueChange={(v) => setRecipe({ ...structured, weekday: Number(v) })}
                    />
                )}

                {structured?.kind === 'monthly' && (
                    <DayOfMonthPicker
                        day={structured.day}
                        onChange={(day) => setRecipe({ ...structured, day })}
                        className={PICKER_TRIGGER_CLASS}
                    />
                )}

                {structured && (
                    <TimePicker
                        variant="field"
                        align="start"
                        hours={structured.hour}
                        minutes={structured.minute}
                        // 5 分钟一档够用；手写进来的非整档分钟（如 7）退回逐分，避免显示被四舍五入
                        minuteStep={structured.minute % 5 === 0 ? 5 : 1}
                        onChange={({ hours, minutes }) =>
                            setRecipe({ ...structured, hour: hours, minute: minutes })
                        }
                        className={PICKER_TRIGGER_CLASS}
                    />
                )}

                {shape === 'custom' && (
                    <TextField
                        className="w-56"
                        value={schedule.cron}
                        onValueChange={(v) => onChange({ cron: v })}
                        placeholder="0 4 * * *"
                        spellCheck={false}
                        autoComplete="off"
                        error={preview.status === 'error' ? preview.message : undefined}
                    />
                )}
            </div>
            {status && <p className="text-2xs leading-snug text-text-tertiary">{status}</p>}
        </div>
    );
}

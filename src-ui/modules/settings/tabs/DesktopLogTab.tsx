// 设置 · 日志 Tab：grid 行布局（与 Bot 日志页同一套行高/列宽策略）。

import type { Ref, RefObject } from 'react';
import { ScrollText } from 'lucide-react';
import type { LogEntry } from '../../../core/domain/events/log-buffer';
import {
    LOG_LEVEL_SHORT,
    levelBarColor,
    levelLabelColor,
    lineTextColor,
} from '../../../shared/log/log-level-display';

const LOG_SURFACE =
    'bg-[color-mix(in_srgb,var(--surface-canvas)_76%,var(--surface-inset)_24%)]';

/** 列表里只显示时分秒，完整时间在 title；避免宽时间列留白造成「和 INFO 隔很远」。 */
function displayTime(timestamp: string): string {
    const t = timestamp.trim();
    const m = t.match(/(\d{2}:\d{2}:\d{2})\s*$/);
    return m ? m[1] : t;
}

type Props = {
    emptyKind: 'loading' | 'error' | 'empty-file' | 'no-match' | 'has';
    entries: LogEntry[];
    fontSize: number;
    viewportRef: RefObject<HTMLDivElement | null>;
    error: string | null;
};

export function DesktopLogTab({ emptyKind, entries, fontSize, viewportRef, error }: Props) {
    const rowPx = Math.max(20, Math.round(fontSize * 1.5));

    return (
        <div
            className={`flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle/50 ${LOG_SURFACE}`}
        >
            {emptyKind !== 'has' ? (
                <LogEmptyState kind={emptyKind} message={error ?? undefined} />
            ) : (
                <div
                    ref={viewportRef as Ref<HTMLDivElement>}
                    role="log"
                    aria-live="off"
                    aria-label="桌面端调试日志"
                    className="scrollbar-hide min-h-0 flex-1 overflow-auto bg-inset/30 px-4 py-4"
                    style={{
                        fontFamily: 'var(--font-mono)',
                        fontSize: `${fontSize}px`,
                        lineHeight: `${rowPx}px`,
                    }}
                >
                    {entries.map((e) => (
                        <DesktopLogLine key={e.id} entry={e} rowPx={rowPx} />
                    ))}
                </div>
            )}
        </div>
    );
}

function DesktopLogLine({ entry, rowPx }: { entry: LogEntry; rowPx: number }) {
    const level = entry.level;
    const label = desktopLevelLabel(entry);
    const timeShort = displayTime(entry.timestamp);
    const timeFull = entry.timestamp || '—';

    return (
        <div
            className="group grid items-center gap-x-1.5 px-1 hover:bg-elevated/80"
            style={{
                height: rowPx,
                gridTemplateColumns: '3px 4.5rem 1.75rem minmax(0, 1fr)',
            }}
        >
            <span
                className="h-3 w-[3px] shrink-0 justify-self-center"
                style={{ background: levelBarColor(level) }}
            />
            <span
                className="truncate text-text-tertiary"
                style={{ fontSize: Math.max(10, rowPx - 7) }}
                title={timeFull}
            >
                {timeShort || '—'}
            </span>
            <span
                className="font-semibold uppercase"
                style={{
                    fontSize: Math.max(9, rowPx - 8),
                    color: levelLabelColor(level),
                }}
            >
                {label}
            </span>
            <span
                className="truncate"
                style={{ color: lineTextColor(level) }}
                title={entry.text}
            >
                {entry.text || ' '}
            </span>
        </div>
    );
}

function desktopLevelLabel(entry: LogEntry): string {
    if (entry.levelTag) {
        const inner = entry.levelTag.replace(/^\[|\]$/g, '').trim().toUpperCase();
        if (inner === 'INFO') return 'INF';
        if (inner.length <= 3) return inner;
        return inner.slice(0, 3);
    }
    return LOG_LEVEL_SHORT[entry.level];
}

function LogEmptyState({
    kind,
    message,
}: {
    kind: 'loading' | 'error' | 'empty-file' | 'no-match';
    message?: string;
}) {
    const copy =
        kind === 'loading'
            ? { title: '正在加载', body: '读取当前会话日志文件…' }
            : kind === 'error'
              ? { title: '加载失败', body: message ?? '无法读取日志文件' }
              : kind === 'empty-file'
                ? { title: '暂无内容', body: '当前日志文件为空' }
                : { title: '没有匹配的行', body: '试试改下搜索关键字或切换等级' };

    return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2.5 py-20 text-center">
            <ScrollText size={20} strokeWidth={1.5} className="text-text-tertiary/60" />
            <p className="text-[13px] font-medium text-text-secondary">{copy.title}</p>
            <p className="max-w-xs text-[12px] leading-relaxed text-text-tertiary">{copy.body}</p>
        </div>
    );
}
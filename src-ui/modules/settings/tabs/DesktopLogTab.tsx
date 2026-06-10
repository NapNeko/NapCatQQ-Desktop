// 设置 · 日志 Tab：canvas 与 inset 混色平面 + 轻边框，避免整块深灰或过亮 field。

import { ScrollText } from 'lucide-react';
import type { DesktopLogViewer } from '../../../hooks/diagnostics/useDesktopLogViewer';

/** 比纯 inset 浅、比 field/画布更有阅读区层次（仅本 Tab）。 */
const LOG_SURFACE =
    'bg-[color-mix(in_srgb,var(--surface-canvas)_76%,var(--surface-inset)_24%)]';

type Props = Pick<
    DesktopLogViewer,
    'emptyKind' | 'displayText' | 'fontSize' | 'viewportRef' | 'error'
>;

export function DesktopLogTab({ emptyKind, displayText, fontSize, viewportRef, error }: Props) {
    return (
        <div
            className={`flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle/50 ${LOG_SURFACE}`}
        >
            {emptyKind !== 'has' ? (
                <LogEmptyState kind={emptyKind} message={error ?? undefined} />
            ) : (
                <pre
                    ref={viewportRef}
                    role="log"
                    aria-live="polite"
                    aria-label="桌面端调试日志"
                    className="scrollbar-hide m-0 min-h-0 flex-1 overflow-auto whitespace-pre px-4 py-3 font-mono text-text-primary/85"
                    style={{
                        fontSize: `${fontSize}px`,
                        fontFamily: 'var(--font-mono)',
                        lineHeight: 1.45,
                    }}
                >
                    {displayText}
                </pre>
            )}
        </div>
    );
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
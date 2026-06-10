// 设置 · 日志 Tab：仅视口（工具条在 SettingsPage sticky 第二行）。

import { ScrollText } from 'lucide-react';
import type { DesktopLogViewer } from '../../../hooks/diagnostics/useDesktopLogViewer';

type Props = Pick<
    DesktopLogViewer,
    'emptyKind' | 'displayText' | 'fontSize' | 'viewportRef' | 'error'
>;

export function DesktopLogTab({ emptyKind, displayText, fontSize, viewportRef, error }: Props) {
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-b-md ring-1 ring-border-subtle ring-t-0 bg-inset">
                {emptyKind !== 'has' ? (
                    <LogEmptyState kind={emptyKind} message={error ?? undefined} />
                ) : (
                    <pre
                        ref={viewportRef}
                        role="log"
                        aria-live="polite"
                        aria-label="桌面端调试日志"
                        className="scrollbar-hide m-0 min-h-0 flex-1 overflow-auto bg-inset px-3 py-2 pb-3 font-mono leading-relaxed text-text-secondary"
                        style={{
                            fontSize: `${fontSize}px`,
                            fontFamily: 'var(--font-mono)',
                        }}
                    >
                        {displayText}
                    </pre>
                )}
            </div>
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
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
            <ScrollText size={22} className="text-text-tertiary opacity-50" />
            <p className="text-[13px] font-semibold text-text-secondary">{copy.title}</p>
            <p className="max-w-sm text-[12px] text-text-tertiary">{copy.body}</p>
        </div>
    );
}
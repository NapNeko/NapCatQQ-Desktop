// 诊断会话列表及小屏键盘焦点边界。

import { useRef, type RefObject } from 'react';
import { Plus, RefreshCw, Settings2, X } from 'lucide-react';
import type { AssistantSessionInfo } from '../../core/domain/assistant/types';
import { assistantStateLabel } from '../../core/domain/assistant/session';
import { BadgeDot, Button } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { sessionStateTone } from './sessionPresentation';

interface Props {
    infos: AssistantSessionInfo[];
    selectedId: string | null;
    loading: boolean;
    preview: boolean;
    open: boolean;
    closeButtonRef: RefObject<HTMLButtonElement>;
    onClose: () => void;
    onRefresh: () => void;
    onSettings: () => void;
    onNew: () => void;
    onSelect: (id: string) => void;
}

function sessionTime(ms: number): string {
    const date = new Date(ms);
    const now = new Date();
    return date.toDateString() === now.toDateString()
        ? date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
        : date.toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' });
}

function SessionItem({
    item,
    selected,
    onSelect,
}: {
    item: AssistantSessionInfo;
    selected: boolean;
    onSelect: () => void;
}) {
    const tone = sessionStateTone[item.state];
    return (
        <button
            type="button"
            aria-current={selected ? 'page' : undefined}
            title={item.title || '新的诊断'}
            onClick={onSelect}
            className={cn(
                'w-full rounded-lg px-3 py-2.5 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand',
                selected ? 'bg-surface shadow-sm' : 'hover:bg-inset/60',
            )}
        >
            <div className="flex min-w-0 items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-text">
                    {item.title || '新的诊断'}
                </span>
                <time
                    className="shrink-0 text-2xs text-text-tertiary"
                    dateTime={new Date(item.updatedAtMs).toISOString()}
                >
                    {sessionTime(item.updatedAtMs)}
                </time>
            </div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5 text-2xs text-text-tertiary">
                <BadgeDot
                    tone={tone}
                    className={cn(
                        'shrink-0',
                        (item.state === 'running' || item.state === 'starting') &&
                            'motion-safe:animate-pulse',
                    )}
                />
                <span className="min-w-0 truncate" title={item.botName}>
                    {item.botName}
                </span>
                {item.state !== 'ready' && (
                    <span
                        className={cn(
                            'shrink-0',
                            tone === 'danger' && 'text-danger',
                            tone === 'warning' && 'text-warning',
                        )}
                    >
                        · {assistantStateLabel[item.state]}
                    </span>
                )}
            </div>
        </button>
    );
}

export function SessionSidebar({
    infos,
    selectedId,
    loading,
    preview,
    open,
    closeButtonRef,
    onClose,
    onRefresh,
    onSettings,
    onNew,
    onSelect,
}: Props) {
    const panelRef = useRef<HTMLElement>(null);
    return (
        <aside
            ref={panelRef}
            id="assistant-sessions"
            aria-label="诊断会话列表"
            onKeyDown={(event) => {
                if (!open) return;
                if (event.key === 'Escape') {
                    event.preventDefault();
                    onClose();
                } else if (event.key === 'Tab') {
                    const buttons = Array.from(
                        panelRef.current?.querySelectorAll<HTMLButtonElement>(
                            'button:not(:disabled)',
                        ) ?? [],
                    ).filter((button) => button.getClientRects().length > 0);
                    const first = buttons[0];
                    const last = buttons[buttons.length - 1];
                    if (event.shiftKey && document.activeElement === first && last) {
                        event.preventDefault();
                        last.focus();
                    } else if (!event.shiftKey && document.activeElement === last && first) {
                        event.preventDefault();
                        first.focus();
                    }
                }
            }}
            className={cn(
                'absolute inset-0 z-20 flex-col bg-canvas px-2.5 pb-3 pt-1 md:static md:z-auto md:flex md:w-[260px] md:shrink-0 md:bg-transparent',
                open ? 'flex' : 'hidden',
            )}
        >
            <div className="flex h-10 shrink-0 items-center gap-1 px-1.5">
                <h1 className="min-w-0 flex-1 truncate text-sm font-semibold text-text">
                    诊断助手
                </h1>
                <Button
                    size="icon"
                    variant="ghost"
                    className="h-7 w-7"
                    aria-label="刷新诊断会话"
                    disabled={loading}
                    onClick={onRefresh}
                >
                    <RefreshCw size={14} className={loading ? 'animate-spin' : undefined} />
                </Button>
                <Button
                    size="icon"
                    variant="ghost"
                    className="h-7 w-7"
                    aria-label="助手设置"
                    onClick={onSettings}
                >
                    <Settings2 size={14} />
                </Button>
                <Button
                    ref={closeButtonRef}
                    size="icon"
                    variant="ghost"
                    className="h-7 w-7 md:hidden"
                    aria-label="收起会话列表"
                    onClick={onClose}
                >
                    <X size={15} />
                </Button>
            </div>
            <button
                type="button"
                onClick={onNew}
                aria-current={!selectedId ? 'page' : undefined}
                className={cn(
                    'mb-3 mt-1 flex h-9 shrink-0 items-center gap-2 rounded-lg border border-dashed px-3 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                    !selectedId
                        ? 'border-brand/40 bg-brand-soft/50 text-brand'
                        : 'border-border text-text-secondary hover:border-brand/40 hover:text-text',
                )}
            >
                <Plus size={15} />
                新建会话
            </button>
            {infos.length > 0 && (
                <div className="flex shrink-0 items-center justify-between px-3 pb-1.5 text-2xs font-medium text-text-tertiary">
                    <span>诊断记录</span>
                    <span>{infos.length}</span>
                </div>
            )}
            <nav className="min-h-0 flex-1 space-y-0.5 overflow-y-auto pb-3" aria-label="选择会话">
                {infos.length === 0 && (
                    <p className="px-3 py-6 text-center text-xs leading-5 text-text-tertiary">
                        {loading ? '加载会话…' : '开始诊断后，会话会保留在这里。'}
                    </p>
                )}
                {infos.map((item) => (
                    <SessionItem
                        key={item.id}
                        item={item}
                        selected={item.id === selectedId}
                        onSelect={() => onSelect(item.id)}
                    />
                ))}
            </nav>
            {preview && (
                <p className="border-t border-border-subtle px-1.5 pt-3 text-2xs leading-5 text-text-tertiary">
                    当前为浏览器界面演示，不会调用真实模型或操作 Bot。
                </p>
            )}
        </aside>
    );
}

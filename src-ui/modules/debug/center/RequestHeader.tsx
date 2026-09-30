// 请求头：动作名（可以直接改、自由输入目录外的名字）、安全分级和几枚标记、简介，
// 右边是收藏、复制请求 JSON、超时。
//
// 参数改过的标签换接口名时先问一句：原地换（参数留着）还是另开一个标签（旧标签原样不动）。
// 两种都不丢东西，但用户心里想的不一样，不替他猜。

import { memo, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { Copy, Star, Timer, X } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Badge, Button, Popover, PopoverAnchor, PopoverContent, PopoverTrigger, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { debugWorkspaceStore, useDebugWorkspaceSelector } from '../../../hooks/debug/debugWorkspaceStore';
import { searchActions } from '../../../core/domain/debug/catalogView';
import type { ParamsParse } from '../../../core/domain/debug/paramsText';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { SAFETY_DOT_CLASS, SAFETY_LABEL, SAFETY_TEXT, SAFETY_TONE } from '../../../core/domain/debug/safety';
import { IconTip, copyWithToast } from './centerParts';
import { markSeeded } from './seedState';
import { prettyJson } from './viewHelpers';

export const DEFAULT_TIMEOUT_S = 60;
const MIN_TIMEOUT_S = 1;
const MAX_TIMEOUT_S = 600;

export interface RequestHeaderProps {
    tab: DebugRequestDraft;
    spec: DebugActionSpec | null;
    specLoading: boolean;
    /** 目录里这一行；说明读失败时分级从这里拿 */
    summary: DebugActionSummary | null;
    /** 目录外的变体（`_async` 等）按哪个原接口认的分级；直接找到时是 null */
    summaryFrom: string | null;
    catalog: readonly DebugActionSummary[];
    target: DebugTarget | null;
    parsed: ParamsParse;
    /** 参数没被用户改过（改动作名时据此决定原地换还是另开） */
    untouched: boolean;
    onSave: () => void;
}

export const RequestHeader = memo(function RequestHeader({
    tab,
    spec,
    specLoading,
    summary,
    summaryFrom,
    catalog,
    target,
    parsed,
    untouched,
    onSave,
}: RequestHeaderProps) {
    const action = tab.action.trim();
    const safety = spec?.safety ?? summary?.safety ?? null;
    const known = !!spec || !!summary;
    const otherPresent = spec ? (spec.other_backend?.present ?? null) : (summary?.other_backend_present ?? null);
    const paramDiff = spec ? (spec.other_backend?.breaking ?? false) : (summary?.param_diff ?? false);
    const stream = spec?.stream ?? summary?.stream ?? false;
    const supported = spec?.supported ?? summary?.supported ?? true;
    const onlyLabel = otherPresent === false && target ? (target.backend === 'napcat' ? '仅 NC' : '仅 SL') : null;

    const rootRef = useRef<HTMLDivElement>(null);
    // 参数改过的标签上敲了新接口名：先记着，等用户选「替换当前标签」还是「新开标签」
    const [renameTo, setRenameTo] = useState<string | null>(null);
    // 外面换了动作（从目录点了别的、撤销……）：待定的这一问作废
    useEffect(() => setRenameTo(null), [tab.action]);

    const commitAction = (name: string) => {
        const next = name.trim();
        if (!next || next === tab.action) return;
        // 没改过参数就原地换动作（参数按新动作重新填），不必问
        if (untouched) {
            debugWorkspaceStore.openAction(next, {});
            return;
        }
        setRenameTo(next);
    };

    const focusActionInput = (scope: Element | null | undefined) =>
        requestAnimationFrame(() => scope?.querySelector<HTMLElement>('[role="combobox"][aria-label="接口名"]')?.focus());

    /** 原地换：参数留在这个标签里，记成已填过，说明读到后不会被当成空标签重新填 */
    const replaceInPlace = (next: string) => {
        setRenameTo(null);
        markSeeded(tab.id, next);
        debugWorkspaceStore.setTabAction(tab.id, next);
        focusActionInput(rootRef.current);
    };

    /**
     * 另开：旧标签不动，新标签带着参数。要让 store 当成「改过的」：先空着打开再写参数
     * （带着参数打开会被记成初始文本，下次从目录点接口就把它连同参数一起顶掉了），并记成已填过
     */
    const openAside = (next: string) => {
        setRenameTo(null);
        // 这个请求头会随旧标签一起卸掉，先记下中栏，焦点交给新标签的接口名
        const column = rootRef.current?.closest('section');
        const id = debugWorkspaceStore.openAction(next, { newTab: true });
        markSeeded(id, next);
        debugWorkspaceStore.setParamsText(id, tab.params_text);
        focusActionInput(column);
    };

    const cancelRename = () => {
        setRenameTo(null);
        focusActionInput(rootRef.current);
    };

    const copyRequest = () => {
        if (parsed.ok) {
            void copyWithToast(prettyJson({ action, params: parsed.value }), '已复制请求 JSON');
        } else {
            void copyWithToast(tab.params_text, '参数 JSON 有错，只复制了参数原文');
        }
    };

    return (
        <div ref={rootRef} className="@container shrink-0 border-b border-border-subtle/70 px-3 pb-2 pt-2">
            <div className="flex min-w-0 items-center gap-1">
                <ActionInput key={tab.id} value={tab.action} catalog={catalog} onCommit={commitAction} />
                <IconTip icon={Star} label="收藏这个请求" disabled={!action} onClick={onSave} />
                <IconTip icon={Copy} label="复制请求 JSON" hint="action + params" disabled={!action} onClick={copyRequest} />
                <TimeoutButton tab={tab} />
            </div>
            <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 pl-1">
                {action && specLoading && !known && <span className="text-2xs text-text-tertiary">正在读取说明…</span>}
                {action && !specLoading && !known && (
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Badge tone="neutral" tabIndex={0}>
                                目录里没有
                            </Badge>
                        </TooltipTrigger>
                        <TooltipContent side="bottom">照样可以发；没有分级时按「有副作用」处理</TooltipContent>
                    </Tooltip>
                )}
                {safety && (
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Badge tone={SAFETY_TONE[safety]} tabIndex={0}>
                                <span aria-hidden className={cn('h-1.5 w-1.5 rounded-full', SAFETY_DOT_CLASS[safety])} />
                                {SAFETY_LABEL[safety]}
                            </Badge>
                        </TooltipTrigger>
                        <TooltipContent side="bottom">{SAFETY_TEXT[safety]}</TooltipContent>
                    </Tooltip>
                )}
                {summaryFrom && (
                    <Badge tone="neutral" title={`目录里没有单列这个变体，分级按 ${summaryFrom} 算`}>
                        按 {summaryFrom} 分级
                    </Badge>
                )}
                {onlyLabel && <Badge tone="info">{onlyLabel}</Badge>}
                {paramDiff && (
                    <Badge tone="neutral" title="两个后端的参数定义有出入，对照表在「文档」里">
                        参数不同
                    </Badge>
                )}
                {stream && (
                    <Badge tone="brand" title="流式接口第二期支持调用，现在只能看文档">
                        流式
                    </Badge>
                )}
                {!supported && (
                    <Badge tone="danger" title="当前 Bot 的版本没有实现它，调用多半返回「不支持的 API」">
                        当前 Bot 不支持
                    </Badge>
                )}
                {(spec?.summary || summary?.summary) && (
                    <span className="min-w-0 flex-1 truncate text-xs text-text-secondary" title={spec?.summary ?? summary?.summary}>
                        {spec?.summary ?? summary?.summary}
                    </span>
                )}
                {!action && <span className="text-xs text-text-tertiary">填一个接口名，或者从左边目录里点一个</span>}
            </div>
            {renameTo && (
                <RenameChoice
                    to={renameTo}
                    onReplace={() => replaceInPlace(renameTo)}
                    onNewTab={() => openAside(renameTo)}
                    onCancel={cancelRename}
                />
            )}
        </div>
    );
});

/** 「参数改过了，换成 X：替换当前标签 / 新开标签」。出现时焦点落在「替换当前标签」上，Esc 取消 */
function RenameChoice({
    to,
    onReplace,
    onNewTab,
    onCancel,
}: {
    to: string;
    onReplace: () => void;
    onNewTab: () => void;
    onCancel: () => void;
}) {
    const firstRef = useRef<HTMLButtonElement>(null);
    useEffect(() => {
        firstRef.current?.focus();
    }, [to]);
    return (
        <div
            role="group"
            aria-label={`参数改过了，换成 ${to} 的方式`}
            onKeyDown={(e) => {
                if (e.key !== 'Escape') return;
                // 拦下来：这一下 Esc 只是不换了，不该再去取消进行中的调用
                e.preventDefault();
                onCancel();
            }}
            className="mt-1.5 flex min-w-0 flex-wrap items-center gap-1.5 rounded-sm border border-brand/30 bg-brand-soft/40 px-2 py-1.5"
        >
            <span className="min-w-0 flex-1 text-xs text-text-secondary">
                参数改过了，换成 <code className="font-mono text-text">{to}</code>：
            </span>
            <Button ref={firstRef} size="sm" variant="primary" onClick={onReplace}>
                替换当前标签
            </Button>
            <Button size="sm" variant="secondary" onClick={onNewTab}>
                新开标签
            </Button>
            <IconTip icon={X} label="不换了" onClick={onCancel} />
        </div>
    );
}

// ---------------------------------------------------------------------------
// 动作名：可编辑的组合框
// ---------------------------------------------------------------------------

function ActionInput({
    value,
    catalog,
    onCommit,
}: {
    value: string;
    catalog: readonly DebugActionSummary[];
    onCommit: (name: string) => void;
}) {
    const [draft, setDraft] = useState(value);
    const [open, setOpen] = useState(false);
    const [hl, setHl] = useState(0);
    const inputRef = useRef<HTMLInputElement>(null);
    const anchorRef = useRef<HTMLDivElement>(null);
    const listId = useId();
    const recent = useDebugWorkspaceSelector((s) => s.ws.recent_actions);
    const editing = draft !== value;
    // 外面换了动作（从目录点开别的、回车提交后 store 更新）：草稿跟着换成新值。
    // 按「上一次看到的外部值」判断，外部值没变时用户正在改的草稿不会被重渲盖掉
    const [seen, setSeen] = useState(value);
    if (seen !== value) {
        setSeen(value);
        setDraft(value);
    }

    const matches = useMemo(
        () => (open ? searchActions([...catalog], editing ? draft : '', recent).slice(0, 12) : []),
        [open, catalog, draft, editing, recent],
    );
    const idx = Math.min(hl, Math.max(0, matches.length - 1));
    const listOpen = open && matches.length > 0;

    /** 放弃草稿，回到标签当前的动作名 */
    const revert = () => {
        setDraft(value);
        setOpen(false);
    };

    /**
     * 提交只有两个入口：回车、点建议。失焦一律当放弃（和 Esc 一样），
     * 所以「点建议 → 失焦」「回车 → 失焦」都只提交一次，也不会把敲了一半的名字当成接口名。
     */
    const commit = (name: string) => {
        setOpen(false);
        const next = name.trim();
        if (next && next !== value) onCommit(next);
        else setDraft(value);
    };

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.nativeEvent.isComposing) return;
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            // 重新打开时从第一项开始，别停在上次收起前高亮的那一项
            if (!open) {
                setOpen(true);
                setHl(0);
            } else setHl(Math.min(matches.length - 1, idx + 1));
        } else if (e.key === 'ArrowUp') {
            if (!listOpen) return;
            e.preventDefault();
            setHl(Math.max(0, idx - 1));
        } else if (e.key === 'Enter') {
            // Ctrl+Enter 时草稿还没换上去：先换接口、这一下不发送，免得发出去的是旧接口
            const mod = e.ctrlKey || e.metaKey;
            if (mod && !editing) return;
            e.preventDefault();
            // 列表开着就挑高亮的那个（用 ↓ 打开、没敲字时也一样）；Ctrl+Enter 只提交敲的字
            const pick = listOpen && !mod ? matches[idx] : undefined;
            commit(pick ? pick.name : draft);
        } else if (e.key === 'Escape') {
            // 只有在改名字或列表开着时才归这里；否则放行，页面上的 Esc 照常取消进行中的调用
            if (!editing && !listOpen) return;
            e.preventDefault();
            revert();
        }
    };

    return (
        <Popover open={listOpen} onOpenChange={(o) => !o && setOpen(false)}>
            <PopoverAnchor asChild>
                <div ref={anchorRef} className="min-w-0 flex-1">
                    <input
                        ref={inputRef}
                        type="text"
                        role="combobox"
                        aria-label="接口名"
                        aria-expanded={listOpen}
                        aria-controls={listOpen ? listId : undefined}
                        aria-activedescendant={listOpen ? `${listId}-${idx}` : undefined}
                        aria-describedby={editing ? `${listId}-hint` : undefined}
                        value={draft}
                        placeholder="接口名，比如 get_login_info"
                        autoFocus={!value}
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(e) => {
                            setDraft(e.target.value);
                            setHl(0);
                            setOpen(true);
                        }}
                        onKeyDown={onKeyDown}
                        onBlur={revert}
                        className={cn(
                            'h-8 w-full min-w-0 rounded-sm border border-transparent bg-transparent px-1.5 font-mono text-[15px] font-semibold text-text outline-none transition-colors',
                            'placeholder:font-sans placeholder:text-[13px] placeholder:font-normal placeholder:text-text-tertiary',
                            'hover:border-border-subtle focus:border-brand focus:bg-field focus:ring-2 focus:ring-inset focus:ring-brand',
                        )}
                    />
                    {editing && (
                        <p id={`${listId}-hint`} className="sr-only">
                            回车换成这个接口，Esc 或点别处放弃
                        </p>
                    )}
                </div>
            </PopoverAnchor>
            <PopoverContent
                align="start"
                sideOffset={4}
                className="w-[var(--radix-popover-trigger-width)] min-w-[280px] max-w-[480px] p-1"
                onOpenAutoFocus={(e) => e.preventDefault()}
                onCloseAutoFocus={(e) => e.preventDefault()}
                onInteractOutside={(e) => {
                    // 点输入框本身不算「点到外面」：列表不该因此收起
                    if (anchorRef.current?.contains(e.target as Node)) e.preventDefault();
                }}
            >
                {/* 列表和底下的提示都不许抢焦点：输入框一失焦草稿就作废了 */}
                <div onMouseDown={(e) => e.preventDefault()}>
                    <div id={listId} role="listbox" aria-label="接口建议" className="max-h-72 overflow-y-auto">
                        {matches.map((a, i) => (
                            <div
                                key={a.name}
                                id={`${listId}-${i}`}
                                role="option"
                                aria-selected={i === idx}
                                onMouseMove={() => i !== idx && setHl(i)}
                                onClick={() => commit(a.name)}
                                className={cn('flex cursor-pointer items-center gap-2 rounded-xs px-2 py-1.5', i === idx && 'bg-inset')}
                            >
                                <span aria-hidden className={cn('h-1.5 w-1.5 shrink-0 rounded-full', SAFETY_DOT_CLASS[a.safety])} />
                                <span className="shrink-0 font-mono text-[12.5px] text-text">{a.name}</span>
                                <span className="min-w-0 truncate text-2xs text-text-tertiary">{a.summary}</span>
                            </div>
                        ))}
                    </div>
                    <p className="border-t border-border-subtle/70 px-2 pt-1.5 text-[10.5px] text-text-tertiary">
                        回车换成选中的；目录里没有的名字也可以直接回车。点别处放弃
                    </p>
                </div>
            </PopoverContent>
        </Popover>
    );
}

// ---------------------------------------------------------------------------
// 超时
// ---------------------------------------------------------------------------

function TimeoutButton({ tab }: { tab: DebugRequestDraft }) {
    const [open, setOpen] = useState(false);
    const seconds = tab.timeout_ms === null ? DEFAULT_TIMEOUT_S : Math.round(tab.timeout_ms / 1000);
    const custom = tab.timeout_ms !== null;
    return (
        <Popover open={open} onOpenChange={setOpen}>
            <Tooltip>
                <TooltipTrigger asChild>
                    <PopoverTrigger asChild>
                        <button
                            type="button"
                            aria-label={`超时 ${seconds} 秒${custom ? '' : '（默认）'}`}
                            className={cn(
                                'inline-flex h-7 shrink-0 items-center gap-1 rounded-sm px-1.5 text-[12px] tabular-nums transition-colors',
                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                'data-[state=open]:bg-inset',
                                custom ? 'text-brand hover:bg-brand-soft' : 'text-text-tertiary hover:bg-inset hover:text-text',
                            )}
                        >
                            <Timer size={14} strokeWidth={2} aria-hidden />
                            {seconds}s
                        </button>
                    </PopoverTrigger>
                </TooltipTrigger>
                <TooltipContent side="bottom">超时（这个标签单独设置）</TooltipContent>
            </Tooltip>
            <PopoverContent align="end" className="w-64">
                <TimeoutEditor tab={tab} onDone={() => setOpen(false)} />
            </PopoverContent>
        </Popover>
    );
}

function TimeoutEditor({ tab, onDone }: { tab: DebugRequestDraft; onDone: () => void }) {
    const initial = tab.timeout_ms === null ? '' : String(Math.round(tab.timeout_ms / 1000));
    const [text, setText] = useState(initial);
    const n = Number(text);
    const valid = text === '' || (/^\d+$/.test(text) && n >= MIN_TIMEOUT_S && n <= MAX_TIMEOUT_S);
    const apply = (raw: string) => {
        setText(raw);
        if (raw === '') debugWorkspaceStore.setTimeout(tab.id, null);
        else if (/^\d+$/.test(raw)) {
            const s = Number(raw);
            if (s >= MIN_TIMEOUT_S && s <= MAX_TIMEOUT_S) debugWorkspaceStore.setTimeout(tab.id, s * 1000);
        }
    };
    return (
        <div className="space-y-2">
            <label className="block text-xs font-medium text-text" htmlFor={`timeout-${tab.id}`}>
                等回包最多等几秒
            </label>
            <div className="flex items-center gap-2">
                <input
                    id={`timeout-${tab.id}`}
                    type="text"
                    inputMode="numeric"
                    autoFocus
                    value={text}
                    placeholder={`${DEFAULT_TIMEOUT_S}（默认）`}
                    onChange={(e) => apply(e.target.value.trim())}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' && valid) {
                            e.preventDefault();
                            onDone();
                        }
                    }}
                    aria-invalid={!valid || undefined}
                    className={cn(
                        'h-8 w-24 rounded-sm border bg-field px-2 font-mono text-sm tabular-nums text-text outline-none focus:ring-2 focus:ring-inset',
                        valid ? 'border-border-subtle focus:border-brand focus:ring-brand' : 'border-danger focus:ring-danger',
                    )}
                />
                <span className="text-xs text-text-secondary">秒</span>
                <Button
                    size="sm"
                    variant="ghost"
                    className="ml-auto"
                    disabled={tab.timeout_ms === null}
                    onClick={() => {
                        apply('');
                        onDone();
                    }}
                >
                    恢复默认
                </Button>
            </div>
            <p className={cn('text-2xs leading-snug', valid ? 'text-text-tertiary' : 'text-danger')}>
                {valid
                    ? `${MIN_TIMEOUT_S}–${MAX_TIMEOUT_S} 秒，默认 ${DEFAULT_TIMEOUT_S}。超时只是不再等，上游可能已经执行了。`
                    : `要填 ${MIN_TIMEOUT_S} 到 ${MAX_TIMEOUT_S} 之间的整数`}
            </p>
        </div>
    );
}

// 响应面板：状态行（或没拿到回包时的错误卡片），「树 / 原文 / 表格」三种看法。
//
// - 同一个标签连发只显示最后一次（useDebugCall 已经保证 runs 里只留最后一次）；新请求在路上时
//   上一次的结果照旧显示、变淡，不先空一下。
// - 结果属于别的 Bot（切了 Bot 之后才回来、或者从历史里打开的）时照样显示，但顶上写明来自哪个 Bot，
//   不会被当成当前 Bot 的结果。
// - 回包里的 group_id / user_id / message_id 点一下：填进当前请求，或者用它新开一个查询；
//   图片地址悬停看缩略图。

import { memo, useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import {
    AlertTriangle,
    Ban,
    Bot,
    Copy,
    Download,
    ImageIcon,
    Package,
    PlugZap,
    RotateCcw,
    Send,
    TimerOff,
    type LucideIcon,
} from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    Button,
    DataTable,
    JsonCodeEditor,
    JsonTree,
    Popover,
    PopoverAnchor,
    PopoverContent,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
    type DataTableCellContext,
    type JsonTreeNodeContext,
} from '../../../shared/ui';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { useScrollMemory } from '../../../hooks/debug/debugScrollMemory';
import { debugWorkspaceStore, type TabRun } from '../../../hooks/debug/debugWorkspaceStore';
import { useDebugActionSpec } from '../../../hooks/debug/useDebugCatalog';
import { useDebugTargets } from '../../../hooks/debug/useDebugTargets';
import { useSaveResponse } from '../../../hooks/debug/useSaveResponse';
import { debugErrorCopy, retcodeHint } from '../../../core/domain/debug/errorCopy';
import { copyAllResponseCopy, formatBytes, isClickableId, tableView } from '../../../core/domain/debug/responseView';
import { buildFormModel, coerceInput } from '../../../core/domain/debug/schemaForm';
import { formatParams, initialParamsText, setParam } from '../../../core/domain/debug/paramsText';
import type { DebugCallOutcome } from '../../../core/ipc/generated/debug/DebugCallOutcome';
import type { DebugError } from '../../../core/ipc/generated/debug/DebugError';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { MOD_KEY_LABEL } from '../TopBar';
import { HISTORY_REQUEST_PREFIX } from '../../../core/domain/debug/historyReplay';
import { IconTip, Segmented, copyWithToast } from './centerParts';
import { ResponseStatus } from './ResponseStatus';
import { markSeeded } from './seedState';
import { FOLLOW_UP_ACTION, isIdValue, isImageUrl, prettyJson, type ClickableIdKey } from './viewHelpers';

type View = 'tree' | 'raw' | 'table';

// 上次选的看法：跨标签、跨路由都沿用（看惯了表格的人不想每次都切）
let preferredView: View = 'tree';

export interface ResponsePanelProps {
    tabId: string;
    run: TabRun | undefined;
    target: DebugTarget | null;
    /** 当前请求能接受的参数名；null 表示不知道（目录外的动作），都让填 */
    fillKeys: ReadonlySet<string> | null;
    /** 当前参数 JSON 是好的（写坏时不能往里填） */
    canFill: boolean;
    onFill: (key: ClickableIdKey, value: number | string) => void;
    onRevealCallChannel: () => void;
    onResend: () => void;
    /** 现在能不能重发（错误卡片上的「重试」） */
    canResend: boolean;
    /** 页面跳转（错误卡片上「去哪解决」的出口按钮）；没给就不画 */
    onNavigate?: (route: AppRoute) => void;
}

export const ResponsePanel = memo(function ResponsePanel({
    tabId,
    run,
    target,
    fillKeys,
    canFill,
    onFill,
    onRevealCallChannel,
    onResend,
    canResend,
    onNavigate,
}: ResponsePanelProps) {
    const m = useMotion();
    const targets = useDebugTargets().data;
    const last = run?.last;
    const inflight = !!run?.inflight;

    // 结果到了读一句；在途时不读（旧结果还在，别让人以为新结果到了）。
    // 这段隐藏文字在「还没有结果」和「有结果」两种画面里放在同一个位置，节点一直在：
    // live region 得先存在、内容再变，读屏才会读，第一次拿到结果时也一样
    const announce =
        !last || inflight
            ? ''
            : last.response.result.kind === 'err'
                ? `没拿到回包：${debugErrorCopy(last.response.result.error).title}`
                : `${last.response.result.outcome.ok ? '成功' : '失败'}，retcode ${last.response.result.outcome.retcode}`;
    const live = (
        <span className="sr-only" role="status">
            {announce}
        </span>
    );

    return (
        <>
            {live}
            {last ? (
                <ResultView
                    tabId={tabId}
                    last={last}
                    inflight={inflight}
                    otherBot={
                        target && last.botId !== target.bot_id
                            ? (targets?.find((t) => t.bot_id === last.botId)?.name ?? last.botId)
                            : null
                    }
                    target={target}
                    fillKeys={fillKeys}
                    canFill={canFill}
                    onFill={onFill}
                    onRevealCallChannel={onRevealCallChannel}
                    onResend={onResend}
                    canResend={canResend}
                    onNavigate={onNavigate}
                    fade={m.enabled}
                />
            ) : (
                <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
                    {inflight ? (
                        <>
                            <Spinner size="md" tone="brand" label="正在等回包" />
                            <p className="text-xs text-text-secondary">正在等回包…</p>
                        </>
                    ) : (
                        <>
                            <span className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-inset text-text-tertiary">
                                <Send size={16} strokeWidth={1.9} aria-hidden />
                            </span>
                            <p className="text-[13px] text-text-secondary">按 {MOD_KEY_LABEL}+Enter 发送，回包显示在这里</p>
                            <p className="max-w-xs text-2xs leading-relaxed text-text-tertiary">
                                点回包里的 group_id / user_id / message_id，可以填进请求或者用它新开查询
                            </p>
                        </>
                    )}
                </div>
            )}
        </>
    );
});

function ResultView({
    tabId,
    last,
    inflight,
    otherBot,
    target,
    fillKeys,
    canFill,
    onFill,
    onRevealCallChannel,
    onResend,
    canResend,
    onNavigate,
    fade,
}: Omit<ResponsePanelProps, 'run'> & {
    last: NonNullable<TabRun['last']>;
    inflight: boolean;
    otherBot: string | null;
    fade: boolean;
}) {
    const fromHistory = last.response.request_id.startsWith(HISTORY_REQUEST_PREFIX);
    const result = last.response.result;
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {(otherBot || fromHistory) && (
                <p className="shrink-0 truncate border-b border-border-subtle/70 bg-info-soft/40 px-3 py-1 text-2xs text-text-secondary">
                    {fromHistory ? '历史记录里的回包' : '上一次的回包'}
                    {otherBot ? `，来自「${otherBot}」，不是当前选中的 Bot` : ''}
                </p>
            )}
            <div
                className={cn('flex min-h-0 flex-1 flex-col', fade && 'transition-opacity duration-200', inflight && 'opacity-50')}
                aria-busy={inflight || undefined}
            >
                {result.kind === 'err' ? (
                    <ErrorCard
                        error={result.error}
                        inflight={inflight}
                        onRevealCallChannel={onRevealCallChannel}
                        onResend={onResend}
                        canResend={canResend}
                        onNavigate={onNavigate}
                    />
                ) : (
                    <OutcomeView
                        key={last.response.request_id}
                        tabId={tabId}
                        requestId={last.response.request_id}
                        action={last.action}
                        fromHistory={fromHistory}
                        outcome={result.outcome}
                        inflight={inflight}
                        target={target}
                        fillKeys={fillKeys}
                        canFill={canFill}
                        onFill={onFill}
                    />
                )}
            </div>
        </div>
    );
}

// ---------------------------------------------------------------------------
// 没拿到回包
// ---------------------------------------------------------------------------

// 出口按钮的图标按目标页面来；不认识的路由（以后加的）不画图标
const EXIT_ICON: Partial<Record<AppRoute, LucideIcon>> = { components: Package, bots: Bot };

function ErrorCard({
    error,
    inflight,
    onRevealCallChannel,
    onResend,
    canResend,
    onNavigate,
}: {
    error: DebugError;
    inflight: boolean;
    onRevealCallChannel: () => void;
    onResend: () => void;
    canResend: boolean;
    onNavigate?: (route: AppRoute) => void;
}) {
    const copy = debugErrorCopy(error);
    const tone = error.kind === 'cancelled' ? 'neutral' : error.kind === 'timeout' ? 'warning' : 'danger';
    const Icon = error.kind === 'cancelled' ? Ban : error.kind === 'timeout' ? TimerOff : AlertTriangle;
    return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-5 py-6">
            <div
                role="alert"
                className={cn(
                    'w-full max-w-md rounded-md border px-4 py-3.5',
                    tone === 'danger' && 'border-danger/30 bg-danger-soft/40',
                    tone === 'warning' && 'border-warning/30 bg-warning-soft/40',
                    tone === 'neutral' && 'border-border-subtle bg-inset',
                )}
            >
                <div className="flex items-start gap-2.5">
                    <Icon
                        size={16}
                        strokeWidth={2.2}
                        aria-hidden
                        className={cn(
                            'mt-0.5 shrink-0',
                            tone === 'danger' ? 'text-danger' : tone === 'warning' ? 'text-warning' : 'text-text-tertiary',
                        )}
                    />
                    <div className="min-w-0 flex-1 space-y-1">
                        <p className="text-[13.5px] font-semibold text-text">{copy.title}</p>
                        {copy.detail && <p className="break-words text-xs leading-relaxed text-text-secondary">{copy.detail}</p>}
                    </div>
                </div>
                <div className="mt-3 flex flex-wrap justify-end gap-2">
                    {copy.channelIssue && (
                        <Button size="sm" variant="secondary" onClick={onRevealCallChannel}>
                            <PlugZap size={12} aria-hidden />
                            查看通道
                        </Button>
                    )}
                    {onNavigate &&
                        copy.exits?.map((exit) => {
                            const ExitIcon = EXIT_ICON[exit.route];
                            return (
                                <Button key={exit.route + exit.label} size="sm" variant="secondary" onClick={() => onNavigate(exit.route)}>
                                    {ExitIcon ? <ExitIcon size={12} aria-hidden /> : null}
                                    {exit.label}
                                </Button>
                            );
                        })}
                    <Button size="sm" variant="secondary" disabled={!canResend || inflight} onClick={onResend}>
                        <RotateCcw size={12} aria-hidden />
                        重试
                    </Button>
                </div>
            </div>
        </div>
    );
}

// ---------------------------------------------------------------------------
// 拿到了回包
// ---------------------------------------------------------------------------

interface IdMenu {
    key: ClickableIdKey;
    value: number | string;
    x: number;
    y: number;
}

function OutcomeView({
    tabId,
    requestId,
    action,
    fromHistory,
    outcome,
    inflight,
    target,
    fillKeys,
    canFill,
    onFill,
}: {
    tabId: string;
    requestId: string;
    action: string;
    /** 历史里重放的：后端没留完整内容，另存不了 */
    fromHistory: boolean;
    outcome: DebugCallOutcome;
    inflight: boolean;
    target: DebugTarget | null;
    fillKeys: ReadonlySet<string> | null;
    canFill: boolean;
    onFill: (key: ClickableIdKey, value: number | string) => void;
}) {
    const table = useMemo(() => (outcome.truncated ? null : tableView(outcome.data)), [outcome]);
    const treeValue = useMemo(
        () => ({
            status: outcome.status,
            retcode: outcome.retcode,
            data: outcome.data,
            message: outcome.message,
            wording: outcome.wording,
        }),
        [outcome],
    );
    const [view, setViewState] = useState<View>(preferredView);
    const setView = (v: View) => {
        preferredView = v;
        setViewState(v);
    };
    // 超大回包没有 data，只能看原文；不是对象数组的回包没有表格
    const effective: View = outcome.truncated ? 'raw' : view === 'table' && !table ? 'tree' : view;
    const rawText = useMemo(
        () => (effective === 'raw' ? (outcome.truncated ? String(outcome.raw) : prettyJson(outcome.raw)) : ''),
        [effective, outcome],
    );

    // 滚动位置按标签记（树是主视图，用约定的键；另两种各记各的）
    const hostRef = useRef<HTMLDivElement>(null);
    const scrollRef = useMemo(
        () =>
            ({
                get current() {
                    return hostRef.current?.querySelector<HTMLElement>('[role="tree"], [role="table"], .cm-scroller') ?? null;
                },
            }) as RefObject<HTMLElement | null>,
        [],
    );
    useScrollMemory(effective === 'tree' ? `response:${tabId}` : `response:${tabId}:${effective}`, scrollRef);
    // 原文视图的编辑器在子组件的 effect 里才建好，补一次渲染让滚动记忆接上它
    const [, bump] = useState(0);
    useEffect(() => {
        if (effective === 'raw') bump((n) => n + 1);
    }, [effective]);

    const [menu, setMenu] = useState<IdMenu | null>(null);
    const pointer = useRef<{ x: number; y: number } | null>(null);

    const openMenu = (keyName: string | undefined, value: unknown) => {
        const key = keyName ? isClickableId(keyName) : null;
        if (!key || !isIdValue(value)) return;
        const host = hostRef.current;
        let at = pointer.current;
        if (!at && host) {
            // 键盘回车：菜单挂在当前选中的那一行上
            const row = host.querySelector<HTMLElement>('[role="treeitem"][aria-selected="true"]');
            const hr = host.getBoundingClientRect();
            const rr = row?.getBoundingClientRect();
            at = rr ? { x: rr.left - hr.left + 48, y: rr.bottom - hr.top } : { x: 24, y: 24 };
        }
        pointer.current = null;
        setMenu({ key, value, x: at?.x ?? 24, y: at?.y ?? 24 });
    };

    // 截断时复制到手的只是开头 256 KiB 的预览，按钮名字和提示都按截不截断来
    const copyAllCopy = copyAllResponseCopy(outcome.truncated);
    const copyAll = () =>
        void copyWithToast(outcome.truncated ? String(outcome.raw) : prettyJson(outcome.raw), copyAllCopy.toast);

    const failureText = !outcome.ok ? outcome.wording || outcome.message : '';
    const hint = !outcome.ok ? retcodeHint(outcome.retcode) : null;

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-b border-border-subtle/70 px-3 py-1.5">
                <div className="flex min-w-0 flex-1 items-center gap-2">
                    {inflight && <Spinner size="xs" tone="brand" label="正在等新的回包" />}
                    <ResponseStatus outcome={outcome} />
                </div>
                <Segmented<View>
                    label="回包的看法"
                    size="xs"
                    value={effective}
                    onChange={setView}
                    options={[
                        { value: 'tree', label: '树', disabled: outcome.truncated, title: outcome.truncated ? '回包太大，只能看原文' : undefined },
                        { value: 'raw', label: '原文' },
                        {
                            value: 'table',
                            label: '表格',
                            disabled: !table,
                            title: table ? undefined : 'data 是一组对象时才能用表格看',
                        },
                    ]}
                />
                <IconTip icon={Copy} label={copyAllCopy.label} size="sm" onClick={copyAll} />
            </div>

            {(failureText || hint) && (
                <div className="shrink-0 border-b border-border-subtle/70 bg-danger-soft/30 px-3 py-1.5 text-xs">
                    {failureText && <p className="break-words text-danger">{failureText}</p>}
                    {hint && <p className="text-2xs text-text-secondary">{hint}</p>}
                </div>
            )}
            {outcome.truncated && (
                <div className="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-b border-border-subtle/70 bg-warning-soft/40 px-3 py-1.5 text-2xs text-text-secondary">
                    <span className="min-w-0 flex-1">
                        回包有 {formatBytes(outcome.size_bytes)}，太大了：树和表格用不了，原文只显示开头的一段。
                        {fromHistory && '历史记录里没有留完整内容。'}
                    </span>
                    {!fromHistory && <SaveFullButton requestId={requestId} action={action} />}
                </div>
            )}

            <div
                ref={hostRef}
                className="relative flex min-h-0 flex-1 flex-col p-2"
                onPointerDownCapture={(e) => {
                    const r = e.currentTarget.getBoundingClientRect();
                    pointer.current = { x: e.clientX - r.left, y: e.clientY - r.top + 6 };
                }}
                onKeyDownCapture={() => {
                    pointer.current = null;
                }}
            >
                {effective === 'tree' && (
                    <JsonTree
                        value={treeValue}
                        defaultExpandDepth={3}
                        onValueClick={(ctx: JsonTreeNodeContext) => openMenu(ctx.key, ctx.value)}
                        valueActions={(ctx) => (isImageUrl(ctx.value) ? <ImagePeek url={ctx.value} /> : null)}
                    />
                )}
                {effective === 'raw' && (
                    <JsonCodeEditor value={rawText} onChange={noop} readOnly ariaLabel="回包原文" />
                )}
                {effective === 'table' && table && (
                    <DataTable
                        columns={table.columns}
                        rows={table.rows}
                        onCellClick={(ctx: DataTableCellContext) => openMenu(ctx.column, ctx.value)}
                    />
                )}

                <Popover open={!!menu} onOpenChange={(o) => !o && setMenu(null)}>
                    <PopoverAnchor asChild>
                        <span
                            aria-hidden
                            className="pointer-events-none absolute h-px w-px"
                            style={{ left: menu?.x ?? 0, top: menu?.y ?? 0 }}
                        />
                    </PopoverAnchor>
                    <PopoverContent align="start" className="w-64 p-1">
                        {menu && (
                            <IdMenuBody
                                menu={menu}
                                target={target}
                                fillKeys={fillKeys}
                                canFill={canFill}
                                onFill={(k, v) => {
                                    onFill(k, v);
                                    setMenu(null);
                                }}
                                onDone={() => setMenu(null)}
                            />
                        )}
                    </PopoverContent>
                </Popover>
            </div>
        </div>
    );
}

const noop = () => undefined;

/** 后端只留最近几次被截断的回包全文；太早的另存会失败，错误条里写原因 */
function SaveFullButton({ requestId, action }: { requestId: string; action: string }) {
    const save = useSaveResponse();
    return (
        <Button
            size="sm"
            variant="secondary"
            disabled={save.isPending}
            onClick={() => save.mutate({ requestId, action })}
            className="shrink-0"
        >
            {save.isPending ? <Spinner size="xs" label="正在另存" /> : <Download size={12} aria-hidden />}
            另存完整内容
        </Button>
    );
}

function IdMenuBody({
    menu,
    target,
    fillKeys,
    canFill,
    onFill,
    onDone,
}: {
    menu: IdMenu;
    target: DebugTarget | null;
    fillKeys: ReadonlySet<string> | null;
    canFill: boolean;
    onFill: (key: ClickableIdKey, value: number | string) => void;
    onDone: () => void;
}) {
    const follow = FOLLOW_UP_ACTION[menu.key];
    // 新开的查询按它自己的参数类型填号（NapCat 的 id 是字符串、SnowLuma 是整数），说明多半已在缓存里
    const followSpec = useDebugActionSpec(target, follow).data ?? null;
    const fillBlocked = !canFill ? '参数 JSON 有错，先改好' : fillKeys && !fillKeys.has(menu.key) ? `当前接口没有 ${menu.key} 参数` : null;

    const openFollowUp = () => {
        let text: string;
        if (followSpec) {
            const field = buildFormModel(followSpec.params_schema).fields.find((f) => f.name === menu.key);
            const value = field ? coerceInput(field, String(menu.value)) : menu.value;
            text = setParam(initialParamsText(followSpec), menu.key, value);
        } else {
            text = formatParams({ [menu.key]: menu.value });
        }
        // 先空着打开、再写参数：这样 store 把它当成「改过的」标签，之后在目录里点别的接口会另开，不会把它顶掉；
        // 同时记成已经填过初始参数，说明读到后不会被当成空标签重新填
        const id = debugWorkspaceStore.openAction(follow, { newTab: true });
        markSeeded(id, follow);
        debugWorkspaceStore.setParamsText(id, text);
        onDone();
    };

    const item =
        'flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-xs transition-colors hover:bg-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent';

    return (
        // 不写 role="menu"：那是给带方向键导航的菜单用的，这里只是几个普通按钮，挂着 role 反而误导读屏
        <div role="group" aria-label={`${menu.key} 能做的事`}>
            <p className="truncate px-2 pb-1 pt-0.5 font-mono text-[11px] text-text-tertiary">
                {menu.key} = <span className="text-text">{String(menu.value)}</span>
            </p>
            <button type="button" className={item} disabled={!!fillBlocked} title={fillBlocked ?? undefined} onClick={() => onFill(menu.key, menu.value)}>
                填进当前请求
            </button>
            {fillBlocked && <p className="px-2 pb-1 text-[10.5px] text-text-tertiary">{fillBlocked}</p>}
            <button type="button" className={item} onClick={openFollowUp}>
                用它新开 <code className="font-mono text-brand">{follow}</code>
            </button>
            <button
                type="button"
                className={item}
                onClick={() => {
                    void copyWithToast(String(menu.value), `已复制 ${menu.key}`);
                    onDone();
                }}
            >
                复制
            </button>
        </div>
    );
}

function ImagePeek({ url }: { url: string }) {
    return (
        <Tooltip delayDuration={120}>
            <TooltipTrigger asChild>
                <button
                    type="button"
                    tabIndex={-1}
                    aria-label="预览图片"
                    className="flex h-[18px] w-[18px] items-center justify-center rounded-xs text-text-tertiary hover:bg-inset hover:text-brand"
                >
                    <ImageIcon size={12} aria-hidden />
                </button>
            </TooltipTrigger>
            <TooltipContent side="left" className="whitespace-normal border border-border-subtle bg-elevated p-1.5">
                <img
                    src={url}
                    alt="图片预览"
                    loading="lazy"
                    referrerPolicy="no-referrer"
                    className="block max-h-[240px] max-w-[240px] rounded-xs object-contain"
                />
            </TooltipContent>
        </Tooltip>
    );
}

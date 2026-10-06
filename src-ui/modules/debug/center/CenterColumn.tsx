// 中栏内容：请求标签页、参数（表单 / JSON）与文档、发送、响应。
//
// 高度由外框给定；标签条、请求头、发送条固定，参数区和响应区上下分，中间的横条可以拖（双击恢复），
// 两块各自在里面滚。快捷键里的发送（Ctrl+Enter）和取消（Esc）归这一栏，
// 标签页的关闭 / 切换 / 找回和命令面板由页面挂。
//
// 一个标签就是一份草稿（动作名 + 参数原文 + 超时 + 可选的通道），全在工作区 store 里；
// 这里切标签时整块按标签 id 重新挂载，字段草稿、折叠状态都不会串到别的标签上。

import {
    memo,
    useCallback,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type KeyboardEvent as ReactKeyboardEvent,
    type PointerEvent as ReactPointerEvent,
    type RefObject,
} from 'react';
import { FilePlus2, Info, Search } from 'lucide-react';
import { Button } from '../../../shared/ui';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import { cn } from '../../../shared/utils/cn';
import { useMotion } from '../../../hooks/preferences/useMotion';
import {
    debugWorkspaceStore,
    useActiveDebugTab,
    useDebugTabs,
    useTabRun,
} from '../../../hooks/debug/debugWorkspaceStore';
import { useDebugActionSpec, useDebugCatalog } from '../../../hooks/debug/useDebugCatalog';
import { useDebugCall } from '../../../hooks/debug/useDebugCall';
import { useDebugChannels } from '../../../hooks/debug/useDebugChannels';
import { cssEase } from '../../../core/design/cssEase';
import {
    initialParamsText,
    parseParamsText,
    setParam,
} from '../../../core/domain/debug/paramsText';
import { countOmittedParams, omittedBlocker } from '../../../core/domain/debug/omittedParams';
import { buildFormModel, coerceInput } from '../../../core/domain/debug/schemaForm';
import { localFilesInParams } from '../../../core/domain/debug/streamActions';
import { validateParams, type ParamIssue } from '../../../core/domain/debug/validate';
import { targetDisplayName } from '../../../core/domain/debug/targetGroups';
import { suggestedRequestName } from '../../../core/domain/debug/collectionsOps';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { useDebugShortcuts } from '../debugShortcuts';
import { MOD_KEY_LABEL } from '../TopBar';
import { DangerConfirmDialog, dangerConfirmSkipped } from '../DangerConfirmDialog';
import { ParamsPane, type ParamsSubTab, type ParamsView } from './ParamsPane';
import { RequestHeader } from './RequestHeader';
import { RequestTabs } from './RequestTabs';
import { ResponsePanel } from './ResponsePanel';
import { SaveRequestDialog } from '../SaveRequestDialog';
import { SendBar } from './SendBar';
import { markSeeded, rememberInitialText, resolveInitialText, wasSeeded } from './seedState';
import { lookupSummary } from '../../../core/domain/debug/catalogView';
import { isBlankParams, paramsDirty, sendBlocker, type ClickableIdKey } from './viewHelpers';

export interface CenterColumnProps {
    /** 当前选中的 Bot。没在运行时文档照看，发送按钮禁用并写「Bot 没在运行」 */
    target: DebugTarget | null;
    /** 顶栏为这个 Bot 选的调用通道（可能是「自动」）；标签页自己指定了通道的以标签为准 */
    callChannel: DebugChannelId;
    /** 打开命令面板（标签条上的「+」用） */
    onOpenPalette: () => void;
    /** 错误卡片上的「查看通道」：把顶栏的调用通道下拉点开 */
    onRevealCallChannel: () => void;
    /** 页面跳转（错误卡片上「去哪解决」的出口按钮）；没给就不画 */
    onNavigate?: (route: AppRoute) => void;
}

// 纯界面偏好，不落盘；切路由再回来、切标签都沿用
let lastSub: ParamsSubTab = 'params';
let lastView: ParamsView = 'form';
const DEFAULT_SPLIT = 0.56;
let splitRatio = DEFAULT_SPLIT;
const MIN_REQUEST_PX = 150;
const MIN_RESPONSE_PX = 110;

const NO_ISSUES: ParamIssue[] = [];
const NO_ACTIONS: DebugActionSummary[] = [];

export const CenterColumn = memo(function CenterColumn({
    target,
    callChannel,
    onOpenPalette,
    onRevealCallChannel,
    onNavigate,
}: CenterColumnProps) {
    const rootRef = useRef<HTMLDivElement>(null);
    const tabs = useDebugTabs();
    const tab = useActiveDebugTab();

    // 切标签时新内容轻轻淡入；进页面的第一帧不播（整页已经有路由动画）
    const shownTab = useRef<string | null>(tab?.id ?? null);
    const animateIn = tab !== null && shownTab.current !== null && shownTab.current !== tab.id;
    useEffect(() => {
        shownTab.current = tab?.id ?? null;
    });

    // 「+」：先给一个空白标签（当前已经是空白的就不再多开），再打开命令面板；
    // 在面板里回车会落在这个空白标签上，按 Esc 关掉面板也能直接在标签里敲接口名
    const newTab = useCallback(() => {
        const { ws } = debugWorkspaceStore.getSnapshot();
        const active = ws.tabs.find((t) => t.id === ws.active_tab);
        if (!active || active.action.trim() !== '' || !isBlankParams(active.params_text))
            debugWorkspaceStore.newTab();
        onOpenPalette();
    }, [onOpenPalette]);

    return (
        <div ref={rootRef} className="@container flex min-h-0 flex-1 flex-col">
            <RequestTabs tabs={tabs} activeId={tab?.id ?? null} target={target} onNewTab={newTab} />
            {tab ? (
                <TabWorkspace
                    key={tab.id}
                    tab={tab}
                    target={target}
                    callChannel={callChannel}
                    scopeRef={rootRef}
                    animateIn={animateIn}
                    onRevealCallChannel={onRevealCallChannel}
                    onNavigate={onNavigate}
                />
            ) : (
                <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
                    <span className="inline-flex h-10 w-10 items-center justify-center rounded-lg bg-brand-soft text-brand">
                        <Search size={18} strokeWidth={1.9} aria-hidden />
                    </span>
                    <p className="text-[13px] text-text-secondary">
                        从左边选一个接口，或按 {MOD_KEY_LABEL}+K 搜索
                    </p>
                    <div className="flex flex-wrap justify-center gap-2">
                        <Button size="sm" variant="secondary" onClick={onOpenPalette}>
                            <Search size={12} aria-hidden />
                            搜索接口
                        </Button>
                        <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => debugWorkspaceStore.newTab()}
                        >
                            <FilePlus2 size={12} aria-hidden />
                            空白请求
                        </Button>
                    </div>
                </div>
            )}
        </div>
    );
});

/** 焦点在别处的输入框里（右栏输入框、左栏搜索框）时，Ctrl+Enter / Esc 是那边的事，这一栏不接 */
function focusAllowsShortcut(scope: RefObject<HTMLElement | null>): boolean {
    // 有对话框开着（收藏、危险确认、命令面板……）时这一栏的快捷键都不接。焦点在对话框里时快捷键 hook 已经按
    // role="dialog" 让路了；这里再按遮罩兜一层：对话框刚开、焦点还没移进去的那一下，Ctrl+Enter 也不能绕过确认
    if (document.querySelector('[data-dialog-overlay]')) return false;
    const el = document.activeElement;
    if (!el || el === document.body) return true;
    if (scope.current?.contains(el)) return true;
    const editable =
        el instanceof HTMLInputElement ||
        el instanceof HTMLTextAreaElement ||
        el instanceof HTMLSelectElement ||
        (el instanceof HTMLElement && el.isContentEditable);
    return !editable;
}

interface TabWorkspaceProps {
    tab: DebugRequestDraft;
    target: DebugTarget | null;
    callChannel: DebugChannelId;
    scopeRef: RefObject<HTMLDivElement | null>;
    animateIn: boolean;
    onRevealCallChannel: () => void;
    onNavigate?: (route: AppRoute) => void;
}

function TabWorkspace({
    tab,
    target,
    callChannel,
    scopeRef,
    animateIn,
    onRevealCallChannel,
    onNavigate,
}: TabWorkspaceProps) {
    const m = useMotion();
    const bodyRef = useRef<HTMLDivElement>(null);
    const action = tab.action.trim();

    // ---- 动作说明、目录里的那一行（说明读失败时，安全分级从这里拿）
    const specQuery = useDebugActionSpec(target, action || null);
    const spec = specQuery.data ?? null;
    const specLoading = specQuery.isLoading;
    const catalog = useDebugCatalog(target).data;
    const { summary, summaryFrom } = useMemo(
        () => lookupSummary(catalog?.actions, action),
        [catalog, action],
    );

    useEffect(() => {
        if (!spec) return;
        // 已经填过初始参数的标签直接回来：换 Bot 后说明变了，但「改没改过」的基准不能再跟着变，
        // 只认当时填进 params_text 的那份（M2）；在这重记会把没动过的标签标成改过
        if (wasSeeded(tab.id, tab.action)) return;
        const initial = initialParamsText(spec);
        rememberInitialText(tab.id, initial);
        markSeeded(tab.id, tab.action);
        const current = debugWorkspaceStore
            .getSnapshot()
            .ws.tabs.find((t) => t.id === tab.id)?.params_text;
        if (current === undefined) return;
        if (isBlankParams(current)) {
            if (current !== initial)
                debugWorkspaceStore.setParamsText(tab.id, initial, { initial: true });
        } else if (current === initial) {
            // 重启后恢复的标签：文本还是初始那份，告诉 store 它没被动过，从目录点别的接口时可以直接顶替
            debugWorkspaceStore.setParamsText(tab.id, current, { initial: true });
        }
    }, [spec, tab.id, tab.action]);

    // ---- 参数
    const parsed = useMemo(() => parseParamsText(tab.params_text), [tab.params_text]);
    const model = useMemo(() => (spec ? buildFormModel(spec.params_schema) : null), [spec]);
    const issues = useMemo(
        () => (spec && parsed.ok ? validateParams(spec.params_schema, parsed.value) : NO_ISSUES),
        [spec, parsed],
    );
    // 和标签上的小点用同一套判断；说明还在读、又没记过初始参数时不知道算不算改过，按「不空就算改过」保守处理：
    // 改动作名时宁可另开标签带走参数，也不原地顶掉
    const initialText = useMemo(
        () => resolveInitialText(tab.id, specQuery.data, specLoading),
        [tab.id, specQuery.data, specLoading],
    );
    const dirty =
        initialText === null
            ? !isBlankParams(tab.params_text)
            : paramsDirty(tab.params_text, initialText);
    const fillKeys = useMemo(
        () => (model ? new Set(model.fields.map((f) => f.name)) : null),
        [model],
    );
    const modelRef = useRef(model);
    modelRef.current = model;

    // ---- 发送
    const run = useTabRun(tab.id);
    const call = useDebugCall();
    const channels = useDebugChannels(target?.bot_id ?? null).data;
    const channel = tab.channel ?? callChannel;
    const safety = spec?.safety ?? summary?.safety ?? null;
    // 历史 / 收藏重放来的参数里可能夹着存盘时瘦身留下的占位文字（超长字符串被换成
    // 「<已省略 N 字节>」，整份过大的收成了摘要）。发出去必然失败，在发送按钮和标签顶部都拦住
    const omittedCount = useMemo(
        () => (parsed.ok ? countOmittedParams(tab.params_text) : 0),
        [parsed.ok, tab.params_text],
    );
    const blocker =
        sendBlocker({
            hasTarget: !!target,
            running: !!target?.running,
            action,
            stream: spec?.stream ?? summary?.stream ?? false,
            localFileCount: parsed.ok ? localFilesInParams(parsed.value).length : 0,
            parseOk: parsed.ok,
            // 目录里有这一行时分级已知，不必等说明
            specLoading: specLoading && !summary,
            channels,
            channel,
        }) ?? omittedBlocker(omittedCount);
    const [confirmOpen, setConfirmOpen] = useState(false);
    const [blockedNonce, setBlockedNonce] = useState(0);

    const fire = () => {
        if (!target || !parsed.ok) return;
        debugWorkspaceStore.pushRecent(action);
        void call.send(tab.id, {
            bot_id: target.bot_id,
            channel,
            action,
            params: parsed.value,
            timeout_ms: tab.timeout_ms,
            origin: 'editor',
        });
    };

    const requestSend = () => {
        if (blocker) {
            setBlockedNonce((n) => n + 1);
            return;
        }
        if (safety === 'dangerous' && target && !dangerConfirmSkipped(target.bot_id, action)) {
            setConfirmOpen(true);
            return;
        }
        fire();
    };
    const sendRef = useRef(requestSend);
    sendRef.current = requestSend;
    // 表单、编辑器里的 Mod-Enter 都走这一个稳定的函数，表单行的 memo 才不会每敲一个字就全失效
    const onSubmit = useCallback(() => sendRef.current(), []);

    const cancel = () => {
        if (!run?.inflight) return false;
        void call.cancel(tab.id);
        return true;
    };

    useDebugShortcuts(
        {
            send: () => {
                if (!focusAllowsShortcut(scopeRef)) return false;
                sendRef.current();
                return true;
            },
            cancel: () => (focusAllowsShortcut(scopeRef) ? cancel() : false),
        },
        { scopeRef },
    );

    // ---- 从回包里填号
    const onFill = useCallback(
        (key: ClickableIdKey, value: number | string) => {
            const cur = debugWorkspaceStore.getSnapshot().ws.tabs.find((t) => t.id === tab.id);
            if (!cur) return;
            const field = modelRef.current?.fields.find((f) => f.name === key);
            const v = field ? coerceInput(field, String(value)) : value;
            const next = setParam(cur.params_text, key, v);
            if (next !== cur.params_text) debugWorkspaceStore.setParamsText(tab.id, next);
        },
        [tab.id],
    );

    // ---- 子页、写法、去改某个参数
    const [sub, setSubState] = useState<ParamsSubTab>(lastSub);
    const setSub = useCallback((s: ParamsSubTab) => {
        lastSub = s;
        setSubState(s);
    }, []);
    const [view, setViewState] = useState<ParamsView>(lastView);
    const setView = useCallback((v: ParamsView) => {
        lastView = v;
        setViewState(v);
    }, []);
    const [focusRequest, setFocusRequest] = useState<{ name: string; nonce: number } | null>(null);
    const jumpToIssue = useCallback(
        (name: string) => {
            setSub('params');
            setView('form');
            setFocusRequest({ name, nonce: Date.now() });
        },
        [setSub, setView],
    );

    const [saveOpen, setSaveOpen] = useState(false);
    const onTabChannelChange = useCallback(
        (c: DebugChannelId | null) => debugWorkspaceStore.setTabChannel(tab.id, c),
        [tab.id],
    );
    const cancelRef = useRef(cancel);
    cancelRef.current = cancel;
    const onCancel = useCallback(() => void cancelRef.current(), []);

    useLayoutEffect(() => {
        const el = bodyRef.current;
        if (!animateIn || !el || !m.enabled || typeof el.animate !== 'function') return;
        const anim = el.animate(
            [
                { opacity: 0, transform: 'translateY(4px)' },
                { opacity: 1, transform: 'none' },
            ],
            { duration: m.duration('fast') * 1000, easing: cssEase(m.ease.enter) },
        );
        return () => anim.cancel();
        // 只在挂上（切到这个标签）时播一次
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const splitHostRef = useRef<HTMLDivElement>(null);
    const topRef = useRef<HTMLDivElement>(null);
    const bottomRef = useRef<HTMLDivElement>(null);

    return (
        <div ref={bodyRef} className="flex min-h-0 flex-1 flex-col">
            <RequestHeader
                tab={tab}
                spec={spec}
                specLoading={specLoading}
                summary={summary}
                summaryFrom={summaryFrom}
                catalog={catalog?.actions ?? NO_ACTIONS}
                target={target}
                callChannel={callChannel}
                parsed={parsed}
                untouched={!dirty}
                onSave={() => setSaveOpen(true)}
            />
            {omittedCount > 0 && (
                <div className="flex shrink-0 items-start gap-1.5 border-b border-border-subtle/70 bg-warning-soft/40 px-2.5 py-1.5 text-2xs leading-snug text-text-secondary">
                    <Info
                        size={12}
                        strokeWidth={2.2}
                        aria-hidden
                        className="mt-px shrink-0 text-warning"
                    />
                    <span>
                        参数里有 {omittedCount}{' '}
                        处超长在存盘时被省略，发出去的只是占位文字；补上原文再发。
                    </span>
                </div>
            )}
            <div ref={splitHostRef} className="flex min-h-0 flex-1 flex-col">
                <div
                    ref={topRef}
                    className="flex min-h-0 flex-col"
                    style={{ flex: `${splitRatio} 1 0px`, minHeight: MIN_REQUEST_PX }}
                >
                    <ParamsPane
                        tab={tab}
                        spec={spec}
                        specLoading={specLoading}
                        model={model}
                        parsed={parsed}
                        issues={issues}
                        target={target}
                        // 说明还在读时不改子页，读到后也不自动跳：来回闪、丢焦点都很烦
                        sub={spec || specLoading ? sub : 'params'}
                        onSubChange={setSub}
                        view={view}
                        onViewChange={setView}
                        dirty={dirty}
                        onSubmit={onSubmit}
                        focusRequest={focusRequest}
                    />
                    <SendBar
                        tabChannel={tab.channel}
                        callChannel={callChannel}
                        channels={channels}
                        onTabChannelChange={onTabChannelChange}
                        blocker={blocker}
                        issues={issues}
                        safety={safety}
                        inflightSince={run?.inflight?.startedAt ?? null}
                        progress={run?.inflight?.progress ?? null}
                        onSend={onSubmit}
                        onCancel={onCancel}
                        onJumpToIssue={jumpToIssue}
                        blockedNonce={blockedNonce}
                    />
                </div>
                <SplitHandle hostRef={splitHostRef} topRef={topRef} bottomRef={bottomRef} />
                <div
                    ref={bottomRef}
                    className="flex min-h-0 flex-col"
                    style={{ flex: `${1 - splitRatio} 1 0px`, minHeight: MIN_RESPONSE_PX }}
                >
                    <ResponsePanel
                        tabId={tab.id}
                        run={run}
                        target={target}
                        fillKeys={fillKeys}
                        canFill={parsed.ok}
                        onFill={onFill}
                        onRevealCallChannel={onRevealCallChannel}
                        onResend={onSubmit}
                        canResend={!blocker}
                        onNavigate={onNavigate}
                    />
                </div>
            </div>

            <SaveRequestDialog
                open={saveOpen}
                onOpenChange={setSaveOpen}
                action={action}
                params={parsed.ok ? parsed.value : null}
                channel={tab.channel}
                suggestedName={suggestedRequestName(action, spec?.summary ?? summary?.summary)}
            />
            {target && (
                <DangerConfirmDialog
                    open={confirmOpen}
                    onOpenChange={setConfirmOpen}
                    botId={target.bot_id}
                    botName={targetDisplayName(target)}
                    action={action}
                    consequenceAction={summaryFrom ?? action}
                    params={parsed.ok ? parsed.value : {}}
                    onConfirm={() => {
                        // 确认框开着的这段时间里 Bot 可能停了、参数可能坏了：真发之前再看一眼
                        if (blocker) setBlockedNonce((n) => n + 1);
                        else fire();
                    }}
                />
            )}
        </div>
    );
}

// ---------------------------------------------------------------------------
// 参数区和回包区之间的横条
// ---------------------------------------------------------------------------

function SplitHandle({
    hostRef,
    topRef,
    bottomRef,
}: {
    hostRef: RefObject<HTMLDivElement | null>;
    topRef: RefObject<HTMLDivElement | null>;
    bottomRef: RefObject<HTMLDivElement | null>;
}) {
    const [, rerender] = useState(0);
    const [dragging, setDragging] = useState(false);
    const drag = useRef<{
        pointerId: number;
        top: number;
        height: number;
        ratio: number;
        frame: number;
    } | null>(null);

    const clampRatio = (r: number, height: number) => {
        if (height <= 0) return r;
        const min = Math.min(0.5, MIN_REQUEST_PX / height);
        const max = Math.max(0.5, 1 - MIN_RESPONSE_PX / height);
        return Math.min(max, Math.max(min, r));
    };
    // 拖动时直接改两块的 flex-grow，不走 React：整栏不重渲，拖起来跟手
    const apply = (r: number) => {
        if (topRef.current) topRef.current.style.flexGrow = String(r);
        if (bottomRef.current) bottomRef.current.style.flexGrow = String(1 - r);
    };
    const commit = (r: number) => {
        splitRatio = r;
        apply(r);
        rerender((n) => n + 1);
    };

    const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        const host = hostRef.current;
        if (!host) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture?.(e.pointerId);
        const rect = host.getBoundingClientRect();
        drag.current = {
            pointerId: e.pointerId,
            top: rect.top,
            height: rect.height,
            ratio: splitRatio,
            frame: 0,
        };
        setDragging(true);
    };
    const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
        const d = drag.current;
        if (!d || e.pointerId !== d.pointerId) return;
        d.ratio = clampRatio((e.clientY - d.top) / d.height, d.height);
        if (!d.frame) {
            d.frame = requestAnimationFrame(() => {
                const cur = drag.current;
                if (!cur) return;
                cur.frame = 0;
                apply(cur.ratio);
            });
        }
    };
    const finish = () => {
        const d = drag.current;
        if (!d) return;
        drag.current = null;
        if (d.frame) cancelAnimationFrame(d.frame);
        setDragging(false);
        commit(d.ratio);
    };
    const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        const height = hostRef.current?.getBoundingClientRect().height ?? 0;
        let next: number | null = null;
        if (e.key === 'ArrowUp') next = splitRatio - 0.05;
        else if (e.key === 'ArrowDown') next = splitRatio + 0.05;
        else if (e.key === 'Home') next = 0;
        else if (e.key === 'End') next = 1;
        if (next === null) return;
        e.preventDefault();
        commit(clampRatio(next, height));
    };

    return (
        <div
            role="separator"
            aria-orientation="horizontal"
            aria-label="调整参数区和回包区的高度"
            aria-valuenow={Math.round(splitRatio * 100)}
            aria-valuemin={0}
            aria-valuemax={100}
            tabIndex={0}
            title="拖动调整高度，双击恢复"
            data-dragging={dragging || undefined}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={finish}
            onPointerCancel={finish}
            onLostPointerCapture={finish}
            onDoubleClick={() => commit(DEFAULT_SPLIT)}
            onKeyDown={onKeyDown}
            className={cn(
                'group relative z-10 h-1.5 shrink-0 cursor-row-resize touch-none select-none outline-none',
                "before:absolute before:inset-x-0 before:-top-1 before:-bottom-1 before:content-['']",
            )}
        >
            <span
                aria-hidden
                className={cn(
                    'pointer-events-none absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-border-subtle transition-colors duration-150',
                    'group-hover:bg-brand/60 group-focus-visible:bg-brand group-data-[dragging]:bg-brand',
                )}
            />
            <span
                aria-hidden
                className="pointer-events-none absolute left-1/2 top-1/2 h-1 w-8 -translate-x-1/2 -translate-y-1/2 rounded-pill bg-border opacity-60 transition-opacity group-hover:opacity-100"
            />
        </div>
    );
}

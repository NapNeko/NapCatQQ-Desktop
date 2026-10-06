// 请求标签条：横向可滚，动作名等宽字体，参数改过的带一个点，× 或中键关闭，「+」新开并打开命令面板。
// 键盘：←/→ 在标签间走，Delete 关当前聚焦的；Ctrl+W / Ctrl+Tab 由页面统一挂。

import {
    memo,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    type KeyboardEvent,
    type WheelEvent,
} from 'react';
import { Plus, X } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { useDebugActionSpec } from '../../../hooks/debug/useDebugCatalog';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { COLUMN_HEADER_CLASS } from '../ColumnFrame';
import { MOD_KEY_LABEL } from '../TopBar';
import { IconTip } from './centerParts';
import { resolveInitialText } from './seedState';
import { paramsDirty } from './viewHelpers';

export interface RequestTabsProps {
    tabs: readonly DebugRequestDraft[];
    activeId: string | null;
    target: DebugTarget | null;
    onNewTab: () => void;
}

export const RequestTabs = memo(function RequestTabs({
    tabs,
    activeId,
    target,
    onNewTab,
}: RequestTabsProps) {
    const listRef = useRef<HTMLDivElement>(null);
    const newTabRef = useRef<HTMLButtonElement>(null);
    // 用键盘（Delete）关掉的：焦点要落到接替它的那个标签上，不能掉回 body
    const focusAfterClose = useRef(false);
    const prevActive = useRef(activeId);

    // 新开 / 切到的标签要露出来
    useEffect(() => {
        // 当前标签被关掉（Ctrl+W、点 ×）时，焦点原来多半在它里面（编辑器、表单、× 按钮），跟着一起没了、掉回 body；
        // 这种情况也把焦点交给接替它的标签。焦点还在别处（左栏、右栏）时不抢
        const closedActive =
            prevActive.current !== null && !tabs.some((t) => t.id === prevActive.current);
        prevActive.current = activeId;
        const lost = !document.activeElement || document.activeElement === document.body;
        const focus = focusAfterClose.current || (closedActive && lost);
        focusAfterClose.current = false;
        if (!activeId) {
            if (focus) newTabRef.current?.focus();
            return;
        }
        const el = listRef.current?.querySelector<HTMLElement>(
            `[data-tab-id="${CSS.escape(activeId)}"]`,
        );
        el?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
        if (focus) el?.querySelector<HTMLElement>('[role="tab"]')?.focus();
    }, [activeId, tabs.length]);

    const closeByKeyboard = useCallback((id: string) => {
        focusAfterClose.current = true;
        debugWorkspaceStore.closeTab(id);
    }, []);

    const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight' && e.key !== 'Home' && e.key !== 'End')
            return;
        const idx = tabs.findIndex((t) => t.id === activeId);
        let next = idx;
        if (e.key === 'ArrowLeft') next = Math.max(0, idx - 1);
        else if (e.key === 'ArrowRight') next = Math.min(tabs.length - 1, idx + 1);
        else if (e.key === 'Home') next = 0;
        else next = tabs.length - 1;
        const tab = tabs[next];
        if (!tab) return;
        e.preventDefault();
        debugWorkspaceStore.setActive(tab.id);
        listRef.current
            ?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(tab.id)}"] [role="tab"]`)
            ?.focus();
    };

    // 竖着滚滚轮时横着走：标签一多，鼠标用户没有别的办法看到后面的
    const onWheel = (e: WheelEvent<HTMLDivElement>) => {
        const el = listRef.current;
        if (!el || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
        el.scrollLeft += e.deltaY;
    };

    return (
        <div className={cn(COLUMN_HEADER_CLASS, 'gap-1 pr-1.5')}>
            <div
                ref={listRef}
                role="tablist"
                aria-label="请求标签"
                onKeyDown={onKeyDown}
                onWheel={onWheel}
                className="flex h-full min-w-0 flex-1 items-center gap-0.5 overflow-x-auto overflow-y-hidden [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
            >
                {tabs.length === 0 && (
                    <span className="px-1 text-2xs text-text-tertiary">还没有打开的请求</span>
                )}
                {tabs.map((tab) => (
                    <TabItem
                        key={tab.id}
                        tab={tab}
                        active={tab.id === activeId}
                        target={target}
                        onKeyboardClose={closeByKeyboard}
                    />
                ))}
            </div>
            <IconTip
                ref={newTabRef}
                icon={Plus}
                label="新请求"
                hint={`打开接口搜索 · ${MOD_KEY_LABEL}+K`}
                onClick={onNewTab}
            />
        </div>
    );
});

const TabItem = memo(function TabItem({
    tab,
    active,
    target,
    onKeyboardClose,
}: {
    tab: DebugRequestDraft;
    active: boolean;
    target: DebugTarget | null;
    onKeyboardClose: (id: string) => void;
}) {
    const action = tab.action.trim();
    // 说明在缓存里（打开标签时已经读过），这里只是拿来算「参数改没改过」
    const specQuery = useDebugActionSpec(target, action || null);
    const spec = specQuery.data;
    // 说明还在读时拿上次记下的初始参数比；一次都没读到过就先不标（拿 `{}` 比会把每个刚打开的标签都标成改过）
    const loading = specQuery.isLoading;
    const initial = useMemo(
        () => resolveInitialText(tab.id, spec, loading),
        [spec, tab.id, loading],
    );
    const dirty = initial !== null && paramsDirty(tab.params_text, initial);
    const close = () => debugWorkspaceStore.closeTab(tab.id);

    return (
        <div data-tab-id={tab.id} className="group/tab relative flex h-8 shrink-0 items-center">
            <button
                type="button"
                role="tab"
                aria-selected={active}
                tabIndex={active ? 0 : -1}
                title={action ? `${action}${dirty ? '（参数改过）' : ''}` : '新请求：还没选接口'}
                onClick={() => debugWorkspaceStore.setActive(tab.id)}
                onMouseDown={(e) => {
                    // 中键按下时浏览器会进入自动滚动，先拦掉
                    if (e.button === 1) e.preventDefault();
                }}
                onAuxClick={(e) => {
                    if (e.button !== 1) return;
                    e.preventDefault();
                    close();
                }}
                onKeyDown={(e) => {
                    if (e.key === 'Delete') {
                        e.preventDefault();
                        onKeyboardClose(tab.id);
                    }
                }}
                className={cn(
                    'flex h-8 min-w-[76px] max-w-[190px] items-center gap-1.5 rounded-sm pl-2.5 pr-7 text-left transition-colors',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand',
                    active
                        ? 'bg-inset text-text'
                        : 'text-text-tertiary hover:bg-inset/60 hover:text-text-secondary',
                )}
            >
                <span
                    className={cn(
                        'min-w-0 truncate text-[12.5px]',
                        action ? 'font-mono' : 'italic text-text-tertiary',
                        active && action && 'font-medium',
                    )}
                >
                    {action || '新请求'}
                </span>
                {dirty && (
                    <span
                        aria-label="参数改过"
                        className="h-1.5 w-1.5 shrink-0 rounded-full bg-brand"
                    />
                )}
            </button>
            <Tooltip>
                <TooltipTrigger asChild>
                    <button
                        type="button"
                        tabIndex={-1}
                        aria-label={`关闭 ${action || '新请求'}`}
                        onClick={close}
                        className={cn(
                            'absolute right-1 inline-flex h-5 w-5 items-center justify-center rounded-xs text-text-tertiary transition-opacity',
                            'hover:bg-elevated hover:text-text',
                            active ? 'opacity-100' : 'opacity-0 group-hover/tab:opacity-100',
                        )}
                    >
                        <X size={12} strokeWidth={2.2} aria-hidden />
                    </button>
                </TooltipTrigger>
                <TooltipContent side="bottom">
                    关闭（{MOD_KEY_LABEL}+W，或中键点标签）
                </TooltipContent>
            </Tooltip>
            {active && (
                <span
                    aria-hidden
                    className="pointer-events-none absolute inset-x-2 bottom-[-3px] h-0.5 rounded-pill bg-brand"
                />
            )}
        </div>
    );
});

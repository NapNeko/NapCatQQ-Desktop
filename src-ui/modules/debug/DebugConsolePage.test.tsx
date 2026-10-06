import type { ReactNode } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugChannels } from '../../core/ipc/generated/debug/DebugChannels';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugWorkspace } from '../../core/ipc/generated/debug/DebugWorkspace';

const service = {
    targets: vi.fn(),
    channels: vi.fn(),
    testChannel: vi.fn(),
    workspace: vi.fn(),
    saveWorkspace: vi.fn(),
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
    receivers: vi.fn(),
    stopReceiver: vi.fn(),
    storageNotices: vi.fn(),
};
const pushErrorBar = vi.fn();
let domainHandlers: Array<(event: unknown) => void> = [];

vi.mock('../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get:
                (_t, key: string) =>
                (...args: unknown[]) =>
                    (service as Record<string, (...a: unknown[]) => unknown>)[key](...args),
        },
    ),
}));
vi.mock('../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: (handler: (event: unknown) => void) => {
        domainHandlers.push(handler);
        return () => {
            domainHandlers = domainHandlers.filter((h) => h !== handler);
        };
    },
}));
vi.mock('../../hooks/ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBar(...args),
}));

import { TooltipProvider } from '../../shared/ui';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { debugWorkspaceStore, defaultWorkspace } from '../../hooks/debug/debugWorkspaceStore';
import { debugEventStore } from '../../hooks/debug/debugEventStore';
import { _resetDebugNavForTests, openDebugConsole } from '../../hooks/debug/debugNav';
import { debugTargetsKey } from '../../hooks/debug/keys';
import { UNSET_COLUMN_WIDTH } from '../../core/domain/debug/workbenchLayout';
import { DebugConsolePage } from './DebugConsolePage';

const SL: DebugTarget = {
    bot_id: 'bot-sl',
    name: '小雪',
    qq_id: 2854196310,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};
const NC: DebugTarget = {
    bot_id: 'bot-nc',
    name: 'NapCat 测试号',
    qq_id: 1919810,
    backend: 'napcat',
    host: { kind: 'remote', server_id: 'srv-1' },
    running: true,
    online: true,
};
const STOPPED: DebugTarget = {
    bot_id: 'bot-off',
    name: '容器里的 NC',
    qq_id: 3141592,
    backend: 'napcat',
    host: { kind: 'docker', server_id: 'srv-1' },
    running: false,
    online: null,
};

function channelsOf(botId: string): DebugChannels {
    return {
        bot_id: botId,
        channels: [
            {
                id: { kind: 'internal' },
                label: '内部通道',
                can_call: true,
                can_receive: true,
                status: { kind: 'available' },
                endpoint: '127.0.0.1:6099/api',
                token_hint: null,
            },
            {
                id: { kind: 'http', name: 'http-default' },
                label: 'HTTP · http-default :3000',
                can_call: true,
                can_receive: false,
                status: { kind: 'unknown' },
                endpoint: '127.0.0.1:3000/',
                token_hint: 'sn***a1',
            },
        ],
        auto_call: { kind: 'internal' },
        auto_events: { kind: 'internal' },
    };
}

function workspace(patch: Partial<DebugWorkspace> = {}): DebugWorkspace {
    return { ...defaultWorkspace(), ...patch };
}

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => {
        resolve = r;
    });
    return { promise, resolve };
}

function renderPage(onNavigate = vi.fn(), prime?: (client: QueryClient) => void) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    prime?.(client);
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return { onNavigate, ...render(<DebugConsolePage onNavigate={onNavigate} />, { wrapper }) };
}

// Radix Popover 打开时会用到这几个 jsdom 没有的 DOM API
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
    // 中栏的 JSON 编辑器（CodeMirror）量文字位置要用
    Range.prototype.getClientRects ??= () =>
        ({
            length: 0,
            item: () => null,
            [Symbol.iterator]: [][Symbol.iterator],
        }) as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect ??= () => new DOMRect();
});

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    pushErrorBar.mockReset();
    domainHandlers = [];
    preferencesStore.setMotionEnabled(false);
    service.targets.mockResolvedValue([SL, NC, STOPPED]);
    service.workspace.mockResolvedValue(workspace());
    service.saveWorkspace.mockResolvedValue(undefined);
    service.channels.mockImplementation(async (botId: string) => channelsOf(botId));
    service.receivers.mockResolvedValue([]);
    service.storageNotices.mockResolvedValue([]);
    service.unsubscribe.mockResolvedValue(undefined);
    let n = 0;
    service.subscribe.mockImplementation(async (botId: string) => ({
        subscription_id: `sub-${++n}`,
        receiver: {
            bot_id: botId,
            source: { kind: 'internal' },
            state: { state: 'connected' },
            buffered: 0,
            dropped_total: 0,
            first_seq: 1,
            viewers: 1,
        },
    }));
});

afterEach(() => {
    cleanup();
    debugWorkspaceStore._reset();
    debugEventStore._reset();
    _resetDebugNavForTests();
    preferencesStore.reset();
});

const botTrigger = () => screen.getByRole('button', { name: /当前 Bot：|选择 Bot/ });

describe('DebugConsolePage', () => {
    it('存储损坏提示：工作区读完之前不发查询，读完取到就立刻显示横幅', async () => {
        // 后端 readStorage 是懒加载：提前取必然拿到空，而且 staleTime: Infinity 之后不会再问（终审 I2）
        const ws = deferred<DebugWorkspace>();
        service.workspace.mockImplementation(() => ws.promise);
        service.storageNotices.mockResolvedValue([
            {
                file: 'history.jsonl',
                moved_to: 'history.jsonl.broken-2026-09-30',
                reason: '第 3,812 行不是合法的 JSON',
            },
        ]);
        renderPage();
        expect(service.storageNotices).not.toHaveBeenCalled();

        ws.resolve(workspace());
        expect(await screen.findByText(/原文件挪到了/)).toBeInTheDocument();
        expect(screen.getByText('history.jsonl.broken-2026-09-30')).toBeInTheDocument();
        expect(service.storageNotices).toHaveBeenCalledTimes(1);
    });

    it('画出三栏，默认选中第一个在跑的 Bot 并开始接收', async () => {
        renderPage();
        expect(await screen.findByRole('region', { name: '接口、收藏与历史' })).toBeInTheDocument();
        expect(screen.getByRole('region', { name: '请求与响应' })).toBeInTheDocument();
        expect(screen.getByRole('region', { name: '事件与聊天' })).toBeInTheDocument();
        expect(screen.getAllByRole('separator')).toHaveLength(2);

        await waitFor(() => expect(botTrigger()).toHaveAccessibleName('当前 Bot：小雪，点击切换'));
        expect(debugWorkspaceStore.getSnapshot().ws.selected_bot).toBe('bot-sl');
        await waitFor(() =>
            expect(service.subscribe).toHaveBeenCalledWith(
                'bot-sl',
                { kind: 'auto' },
                expect.any(Function),
            ),
        );
        expect(
            await screen.findByRole('button', { name: '调用通道：自动（内部通道）' }),
        ).toBeInTheDocument();
    });

    it('Bot 选择器按宿主分组，切 Bot 后放掉旧的、订新的', async () => {
        const user = userEvent.setup();
        renderPage();
        await waitFor(() => expect(botTrigger()).toHaveAccessibleName('当前 Bot：小雪，点击切换'));
        await waitFor(() => expect(service.subscribe).toHaveBeenCalledTimes(1));

        await user.click(botTrigger());
        const list = await screen.findByRole('listbox', { name: 'Bot 列表' });
        expect(within(list).getByRole('group', { name: '本机' })).toBeInTheDocument();
        expect(within(list).getByRole('group', { name: '远端 · srv-1' })).toBeInTheDocument();
        expect(within(list).getByRole('group', { name: 'Docker · srv-1' })).toBeInTheDocument();
        expect(within(list).getByText('未运行')).toBeInTheDocument();

        await user.click(within(list).getByRole('option', { name: /NapCat 测试号/ }));

        expect(debugWorkspaceStore.getSnapshot().ws.selected_bot).toBe('bot-nc');
        await waitFor(() =>
            expect(botTrigger()).toHaveAccessibleName('当前 Bot：NapCat 测试号，点击切换'),
        );
        await waitFor(() =>
            expect(service.subscribe).toHaveBeenLastCalledWith(
                'bot-nc',
                { kind: 'auto' },
                expect.any(Function),
            ),
        );
        expect(service.unsubscribe).toHaveBeenCalledWith('sub-1');
    });

    it('没在跑的 Bot 不去订事件，右栏顶上写明', async () => {
        service.workspace.mockResolvedValue(workspace({ selected_bot: 'bot-off' }));
        renderPage();
        expect(await screen.findByText('Bot 没在运行，启动后会自动开始接收')).toBeInTheDocument();
        await waitFor(() =>
            expect(botTrigger()).toHaveAccessibleName('当前 Bot：容器里的 NC，点击切换'),
        );
        expect(service.subscribe).not.toHaveBeenCalled();
    });

    it('收起右栏后聊天不见了，落进工作区；再点展开回来', async () => {
        const user = userEvent.setup();
        renderPage();
        expect(await screen.findByRole('region', { name: '事件与聊天' })).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: '收起右栏（聊天）' }));
        expect(screen.queryByRole('region', { name: '事件与聊天' })).not.toBeInTheDocument();
        expect(screen.getAllByRole('separator')).toHaveLength(1);
        expect(debugWorkspaceStore.getSnapshot().ws.layout.right_collapsed).toBe(true);
        // 收起的只是画面，事件照样在收
        expect(service.unsubscribe).not.toHaveBeenCalled();

        await user.click(screen.getByRole('button', { name: '展开右栏（聊天）' }));
        expect(screen.getByRole('region', { name: '事件与聊天' })).toBeInTheDocument();
    });

    it('左栏收成窄边，点窄边上的图标切面板并展开', async () => {
        const user = userEvent.setup();
        renderPage();
        await screen.findByRole('region', { name: '接口、收藏与历史' });

        await user.click(screen.getByRole('button', { name: '收起左栏' }));
        expect(screen.queryByRole('region', { name: '接口、收藏与历史' })).not.toBeInTheDocument();
        const rail = screen.getByRole('navigation', { name: '左栏（已收起）' });
        // 点的按钮没了，焦点落到窄边的「展开」上
        expect(within(rail).getByRole('button', { name: '展开左栏' })).toHaveFocus();

        await user.click(within(rail).getByRole('button', { name: '历史' }));
        expect(screen.getByRole('region', { name: '接口、收藏与历史' })).toBeInTheDocument();
        expect(screen.getByRole('tab', { name: '历史' })).toHaveAttribute('aria-selected', 'true');
        expect(screen.getByRole('tab', { name: '历史' })).toHaveFocus();
        expect(debugWorkspaceStore.getSnapshot().ws.layout.left_collapsed).toBe(false);
    });

    it('分隔条用键盘调宽，落进工作区并夹在上下限里', async () => {
        renderPage();
        await screen.findByRole('region', { name: '接口、收藏与历史' });
        const [leftSplitter] = screen.getAllByRole('separator');
        fireEvent.keyDown(leftSplitter, { key: 'ArrowRight' });
        expect(debugWorkspaceStore.getSnapshot().ws.layout.left_width).toBe(256);
        fireEvent.keyDown(leftSplitter, { key: 'End' });
        expect(debugWorkspaceStore.getSnapshot().ws.layout.left_width).toBe(360);
        // 恢复默认记成「没拖过」，窄工作台上照样换窄的默认值
        fireEvent.doubleClick(leftSplitter);
        expect(debugWorkspaceStore.getSnapshot().ws.layout.left_width).toBe(UNSET_COLUMN_WIDTH);
        expect(leftSplitter).toHaveAttribute('aria-valuenow', '240');
    });

    it('从 Bot 卡片跳进来时选中带过来的 Bot（页面已挂着也行）', async () => {
        renderPage();
        await waitFor(() => expect(botTrigger()).toHaveAccessibleName('当前 Bot：小雪，点击切换'));
        act(() => openDebugConsole('bot-nc'));
        await waitFor(() =>
            expect(botTrigger()).toHaveAccessibleName('当前 Bot：NapCat 测试号，点击切换'),
        );
    });

    it('调用通道换成 HTTP 后按 Bot 记住', async () => {
        const user = userEvent.setup();
        renderPage();
        const trigger = await screen.findByRole('button', { name: '调用通道：自动（内部通道）' });
        await user.click(trigger);
        const group = await screen.findByRole('radiogroup', { name: '调用通道' });
        expect(within(group).getByRole('radio', { name: /^自动/ })).toHaveAttribute(
            'aria-checked',
            'true',
        );
        await user.click(within(group).getByRole('radio', { name: /HTTP · http-default :3000/ }));
        expect(debugWorkspaceStore.getSnapshot().ws.channel_choice['bot-sl']).toEqual({
            call: { kind: 'http', name: 'http-default' },
            events: { kind: 'auto' },
        });
        expect(
            await screen.findByRole('button', { name: '调用通道：HTTP · http-default' }),
        ).toBeInTheDocument();
    });

    it('通道全都不可用时，调用通道下拉给「去组件页 / 机器人页」的出口，点完顺手关上下拉', async () => {
        const user = userEvent.setup();
        service.channels.mockResolvedValue({
            bot_id: 'bot-sl',
            channels: [],
            auto_call: null,
            auto_events: null,
        });
        const { onNavigate } = renderPage();
        await user.click(
            await screen.findByRole('button', { name: '调用通道：自动（没有可用通道）' }),
        );
        let group = await screen.findByRole('radiogroup', { name: '调用通道' });
        expect(within(group).getByText('眼下没有能用的通道')).toBeInTheDocument();

        await user.click(within(group).getByRole('button', { name: '去「组件」页装/修运行时' }));
        expect(onNavigate).toHaveBeenCalledWith('components');
        await waitFor(() =>
            expect(screen.queryByRole('radiogroup', { name: '调用通道' })).not.toBeInTheDocument(),
        );

        await user.click(screen.getByRole('button', { name: '调用通道：自动（没有可用通道）' }));
        group = await screen.findByRole('radiogroup', { name: '调用通道' });
        await user.click(within(group).getByRole('button', { name: '去「机器人」页开 WS 服务' }));
        expect(onNavigate).toHaveBeenCalledWith('bots');
    });

    it('页面没给 onNavigate 时出口按钮不画，文案照旧', async () => {
        const user = userEvent.setup();
        service.channels.mockResolvedValue({
            bot_id: 'bot-sl',
            channels: [],
            auto_call: null,
            auto_events: null,
        });
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        const wrapper = ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>
                <TooltipProvider>{children}</TooltipProvider>
            </QueryClientProvider>
        );
        render(<DebugConsolePage />, { wrapper });
        await user.click(
            await screen.findByRole('button', { name: '调用通道：自动（没有可用通道）' }),
        );
        const group = await screen.findByRole('radiogroup', { name: '调用通道' });
        expect(within(group).getByText('眼下没有能用的通道')).toBeInTheDocument();
        expect(
            within(group).queryByRole('button', { name: '去「组件」页装/修运行时' }),
        ).not.toBeInTheDocument();
        expect(
            within(group).queryByRole('button', { name: '去「机器人」页开 WS 服务' }),
        ).not.toBeInTheDocument();
    });

    it('标签页快捷键：Ctrl+W 关当前、Ctrl+Shift+T 找回、Ctrl+Tab 切下一个', async () => {
        service.workspace.mockResolvedValue(
            workspace({
                tabs: [
                    {
                        id: 't1',
                        action: 'get_status',
                        params_text: '{}',
                        timeout_ms: null,
                        channel: null,
                    },
                    {
                        id: 't2',
                        action: 'get_login_info',
                        params_text: '{}',
                        timeout_ms: null,
                        channel: null,
                    },
                ],
                active_tab: 't1',
            }),
        );
        renderPage();
        await screen.findByRole('region', { name: '请求与响应' });

        fireEvent.keyDown(document.body, { key: 'Tab', ctrlKey: true });
        expect(debugWorkspaceStore.getSnapshot().ws.active_tab).toBe('t2');

        fireEvent.keyDown(document.body, { key: 'w', ctrlKey: true });
        expect(debugWorkspaceStore.getSnapshot().ws.tabs.map((t) => t.id)).toEqual(['t1']);

        fireEvent.keyDown(document.body, { key: 'T', ctrlKey: true, shiftKey: true });
        expect(debugWorkspaceStore.getSnapshot().ws.tabs.map((t) => t.action)).toEqual([
            'get_status',
            'get_login_info',
        ]);
    });

    it('Ctrl+K 打开命令面板；面板开着时 Ctrl+W 不关背后的标签，Esc 关面板', async () => {
        service.workspace.mockResolvedValue(
            workspace({
                tabs: [
                    {
                        id: 't1',
                        action: 'get_status',
                        params_text: '{}',
                        timeout_ms: null,
                        channel: null,
                    },
                ],
                active_tab: 't1',
            }),
        );
        const user = userEvent.setup();
        renderPage();
        await screen.findByRole('region', { name: '请求与响应' });
        fireEvent.keyDown(document.body, { key: 'k', ctrlKey: true });
        const palette = await screen.findByRole('dialog', { name: '搜索接口' });
        const input = within(palette).getByRole('combobox', { name: '搜索接口' });
        await waitFor(() => expect(input).toHaveFocus());

        fireEvent.keyDown(input, { key: 'w', ctrlKey: true });
        expect(debugWorkspaceStore.getSnapshot().ws.tabs.map((t) => t.id)).toEqual(['t1']);

        await user.keyboard('{Escape}');
        await waitFor(() =>
            expect(screen.queryByRole('dialog', { name: '搜索接口' })).not.toBeInTheDocument(),
        );
    });

    it('标签条上的「+」先给一个空白标签再打开命令面板', async () => {
        renderPage();
        await screen.findByRole('region', { name: '请求与响应' });
        fireEvent.click(screen.getByRole('button', { name: /新请求/ }));
        expect(await screen.findByRole('dialog', { name: '搜索接口' })).toBeInTheDocument();
        expect(debugWorkspaceStore.getSnapshot().ws.tabs).toHaveLength(1);
        expect(debugWorkspaceStore.getSnapshot().ws.tabs[0].action).toBe('');
    });

    it('收起 / 展开左栏时中栏内容从原位置滑过去（只动 transform）；关了动画就不滑', async () => {
        const played: Array<{ el: Element; frames: Keyframe[] }> = [];
        const animate = vi.fn(function (this: Element, frames: Keyframe[]) {
            played.push({ el: this, frames });
            return { cancel() {} } as unknown as Animation;
        });
        const originalAnimate = Element.prototype.animate;
        Element.prototype.animate = animate as unknown as typeof Element.prototype.animate;
        // 中栏（含它的内容节点）的左边缘：左栏展开时在 244，收起成窄边后在 48。
        // 滑动记录量的是 [data-column-body] 里的内容节点（带 transform 的视觉位置），不是 section 自己
        vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
            this: HTMLElement,
        ) {
            const collapsed = debugWorkspaceStore.getSnapshot().ws.layout.left_collapsed;
            const left = this.closest('[aria-label="请求与响应"]') ? (collapsed ? 48 : 244) : 0;
            return new DOMRect(left, 0, 500, 600);
        });
        try {
            preferencesStore.setMotionEnabled(true);
            renderPage();
            const center = await screen.findByRole('region', { name: '请求与响应' });
            fireEvent.click(screen.getByRole('button', { name: '收起左栏' }));
            await waitFor(() =>
                expect(debugWorkspaceStore.getSnapshot().ws.layout.left_collapsed).toBe(true),
            );
            const slide = played.find(
                (p) =>
                    center.contains(p.el) &&
                    String(p.frames[0]?.transform).startsWith('translateX(196px)'),
            );
            expect(slide).toBeDefined();
            expect(Object.keys(slide?.frames[0] ?? {})).toEqual(['transform']);

            played.length = 0;
            preferencesStore.setMotionEnabled(false);
            fireEvent.click(screen.getByRole('button', { name: '展开左栏' }));
            await waitFor(() =>
                expect(debugWorkspaceStore.getSnapshot().ws.layout.left_collapsed).toBe(false),
            );
            expect(played.some((p) => center.contains(p.el))).toBe(false);
        } finally {
            Element.prototype.animate = originalAnimate;
        }
    });

    it('离开页面时退掉事件订阅（后端按「没人看」算空闲）', async () => {
        const { unmount } = renderPage();
        await waitFor(() => expect(service.subscribe).toHaveBeenCalledTimes(1));
        await waitFor(() =>
            expect(debugEventStore.getSnapshot().bots['bot-sl']?.subscriptionId).toBe('sub-1'),
        );
        expect(service.unsubscribe).not.toHaveBeenCalled();
        unmount();
        expect(service.unsubscribe).toHaveBeenCalledWith('sub-1');
    });

    it('一个 Bot 都没有时给空状态和去机器人页的入口', async () => {
        const user = userEvent.setup();
        service.targets.mockResolvedValue([]);
        const { onNavigate } = renderPage();
        expect(await screen.findByText('还没有 Bot')).toBeInTheDocument();
        expect(screen.queryByRole('region', { name: '请求与响应' })).not.toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '去「机器人」页' }));
        expect(onNavigate).toHaveBeenCalledWith('bots');
    });

    it('正在接收的 Bot 数显示在顶栏，能逐个停', async () => {
        const user = userEvent.setup();
        service.receivers.mockResolvedValue([
            {
                bot_id: 'bot-sl',
                source: { kind: 'internal' },
                state: { state: 'connected' },
                buffered: 1234,
                dropped_total: 0,
                first_seq: 1,
                viewers: 1,
            },
            {
                bot_id: 'bot-nc',
                source: { kind: 'ws', name: 'ws-default' },
                state: { state: 'reconnecting', attempt: 2, retry_in_ms: 2000 },
                buffered: 5,
                dropped_total: 3,
                first_seq: 1,
                viewers: 0,
            },
        ]);
        service.stopReceiver.mockResolvedValue(undefined);
        renderPage();
        await user.click(
            await screen.findByRole('button', { name: '正在接收 2 个 Bot 的事件，点击查看' }),
        );
        expect(await screen.findByText('缓冲 1,234 条')).toBeInTheDocument();
        expect(screen.getByText('重连中（第 2 次，2 秒后）')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: '停止接收 NapCat 测试号 的事件' }));
        await waitFor(() => expect(service.stopReceiver).toHaveBeenCalledWith('bot-nc'));
    });

    it('一个都没在收时不显示接收指示', async () => {
        renderPage();
        await screen.findByRole('region', { name: '请求与响应' });
        await waitFor(() => expect(service.receivers).toHaveBeenCalled());
        expect(screen.queryByRole('button', { name: /正在接收/ })).not.toBeInTheDocument();
    });

    it('进页面前就带了待选 Bot、工作区晚到、缓存里的列表还是旧的：刷新回来后仍选中带进来的 Bot', async () => {
        const ws = deferred<DebugWorkspace>();
        const fresh = deferred<DebugTarget[]>();
        service.workspace.mockReturnValue(ws.promise);
        service.targets.mockReturnValue(fresh.promise);
        openDebugConsole('bot-nc');
        // 缓存里是一秒前的旧列表，还没有 bot-nc；30 秒的 staleTime 内 react-query 自己不会重拉
        renderPage(vi.fn(), (client) =>
            client.setQueryData(debugTargetsKey, [SL], { updatedAt: Date.now() - 1000 }),
        );

        await act(async () => ws.resolve(workspace()));
        // 工作区到了、列表还是旧的：不能拿旧列表把 bot-nc 换成默认的小雪
        expect(debugWorkspaceStore.getSnapshot().ws.selected_bot).toBe('bot-nc');

        await act(async () => fresh.resolve([SL, NC]));
        await waitFor(() =>
            expect(botTrigger()).toHaveAccessibleName('当前 Bot：NapCat 测试号，点击切换'),
        );
        expect(debugWorkspaceStore.getSnapshot().ws.selected_bot).toBe('bot-nc');
        await waitFor(() =>
            expect(service.subscribe).toHaveBeenCalledWith(
                'bot-nc',
                { kind: 'auto' },
                expect.any(Function),
            ),
        );
        expect(service.subscribe).not.toHaveBeenCalledWith(
            'bot-sl',
            expect.anything(),
            expect.anything(),
        );
    });

    it('刷新回来还是找不到带进来的 Bot，才退回默认的', async () => {
        openDebugConsole('bot-gone');
        renderPage();
        await waitFor(() => expect(botTrigger()).toHaveAccessibleName('当前 Bot：小雪，点击切换'));
        expect(debugWorkspaceStore.getSnapshot().ws.selected_bot).toBe('bot-sl');
    });

    it('选中的 Bot 从停到跑，自动开始接收', async () => {
        service.workspace.mockResolvedValue(workspace({ selected_bot: 'bot-off' }));
        renderPage();
        await screen.findByText('Bot 没在运行，启动后会自动开始接收');
        expect(service.subscribe).not.toHaveBeenCalled();

        service.targets.mockResolvedValue([SL, NC, { ...STOPPED, running: true, online: true }]);
        await act(async () => {
            for (const h of domainHandlers)
                h({ kind: 'bot_state_changed', snapshot: { bot_id: 'bot-off' } });
        });

        await waitFor(() =>
            expect(service.subscribe).toHaveBeenCalledWith(
                'bot-off',
                { kind: 'auto' },
                expect.any(Function),
            ),
        );
        expect(screen.queryByText('Bot 没在运行，启动后会自动开始接收')).not.toBeInTheDocument();
    });

    it('Bot 多到出搜索框时，回车选列表里看到的第一行（本机那组在最前）', async () => {
        const user = userEvent.setup();
        const remote = Array.from({ length: 6 }, (_, i) => ({
            ...NC,
            bot_id: `r${i}`,
            name: `远端 ${i}`,
        }));
        const local = { ...SL, bot_id: 'local-last', name: '本机那个' };
        service.targets.mockResolvedValue([...remote, local]);
        service.workspace.mockResolvedValue(workspace({ selected_bot: 'r0' }));
        renderPage();
        await waitFor(() =>
            expect(botTrigger()).toHaveAccessibleName('当前 Bot：远端 0，点击切换'),
        );

        await user.click(botTrigger());
        const search = await screen.findByRole('textbox', { name: '搜索 Bot' });
        await waitFor(() => expect(search).toHaveFocus());
        await user.keyboard('{Enter}');
        expect(debugWorkspaceStore.getSnapshot().ws.selected_bot).toBe('local-last');
    });
});

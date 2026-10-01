import type { ReactNode } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugHistoryEntry } from '../../../core/ipc/generated/debug/DebugHistoryEntry';
import type { DebugHistoryQuery } from '../../../core/ipc/generated/debug/DebugHistoryQuery';
import type { DebugHistorySummary } from '../../../core/ipc/generated/debug/DebugHistorySummary';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

const service: Record<string, ReturnType<typeof vi.fn>> = {
    history: vi.fn(),
    historyEntry: vi.fn(),
    clearHistory: vi.fn(),
    collections: vi.fn(),
    saveCollections: vi.fn(),
    catalog: vi.fn(),
};

vi.mock('../../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get: (_t, key: string) => (...args: unknown[]) => {
                const fn = service[key];
                if (!fn) throw new Error(`没有模拟 service.${key}`);
                return fn(...args);
            },
        },
    ),
}));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { revealLeftSearch, setLeftSearchOpen } from '../leftPanels';
import { LeftColumn } from './LeftColumn';

const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).classList.contains('overflow-y-auto') ? 600 : 30;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', { configurable: true, get: () => 280 });
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
});
afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
});

const SL: DebugTarget = {
    bot_id: 'bot-sl',
    name: '小雪',
    qq_id: 2854196310,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};

function full(id: string, patch: Partial<DebugHistoryEntry> = {}): DebugHistoryEntry {
    return {
        id,
        at_ms: Date.now() - 3 * 60_000,
        bot_id: 'bot-sl',
        bot_name: '小雪',
        backend: 'snowluma',
        channel: { kind: 'internal' },
        origin: 'editor',
        action: 'get_group_list',
        params: { no_cache: true },
        ok: true,
        retcode: 0,
        error: null,
        elapsed_ms: 128,
        response: { status: 'ok', retcode: 0, data: [{ group_id: 1 }], message: '', wording: '' },
        response_truncated: false,
        ...patch,
    };
}

function summary(e: DebugHistoryEntry): DebugHistorySummary {
    return {
        id: e.id,
        at_ms: e.at_ms,
        bot_id: e.bot_id,
        bot_name: e.bot_name,
        backend: e.backend,
        channel: e.channel,
        origin: e.origin,
        action: e.action,
        ok: e.ok,
        retcode: e.retcode,
        elapsed_ms: e.elapsed_ms,
        error_kind: e.error ? e.error.kind : null,
    };
}

const ENTRIES = [
    full('h1'),
    full('h2', { action: 'send_group_msg', ok: false, retcode: 1400, params: { group_id: 1, message: 'hi' } }),
    full('h3', { action: 'get_login_info', ok: false, retcode: null, error: { kind: 'timeout', ms: 60000 }, response: null }),
];

const COLLECTIONS: DebugCollections = { version: 1, folders: [], requests: [] };

function renderHistory(target: DebugTarget | null = SL) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(<LeftColumn target={target} panel="history" />, { wrapper });
}

const lastQuery = (): DebugHistoryQuery => service.history.mock.calls.at(-1)?.[0] as DebugHistoryQuery;
const row = (text: string) => screen.getByText(text).closest('[role="option"]') as HTMLElement;

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    preferencesStore.setMotionEnabled(false);
    debugWorkspaceStore._reset();
    // 搜索条默认收着（开关状态记在模块里）：每个用例自己决定要不要展开
    setLeftSearchOpen('history', false);
    service.history.mockImplementation(async (q: DebugHistoryQuery) => {
        const matched = ENTRIES.filter((e) => q.ok === null || e.ok === q.ok);
        return { entries: matched.slice(q.offset, q.offset + q.limit).map(summary), total: matched.length };
    });
    service.historyEntry.mockImplementation(async (id: string) => ENTRIES.find((e) => e.id === id) ?? null);
    service.clearHistory.mockResolvedValue(undefined);
    // 收藏按「磁盘」记着：保存后重拉拿到的是存进去的那份
    let disk = COLLECTIONS;
    service.collections.mockImplementation(async () => disk);
    service.saveCollections.mockImplementation(async (next: DebugCollections) => {
        disk = next;
    });
    service.catalog.mockResolvedValue({
        backend: 'snowluma',
        source: 'live',
        snapshot_version: '0.9.0',
        actions: [
            {
                name: 'send_group_msg',
                aliases: [],
                summary: '发送群消息',
                category: 'message',
                safety: 'side_effect',
                stream: false,
                supported: true,
                other_backend_present: true,
                param_diff: false,
            },
        ],
    });
});

describe('调用历史', () => {
    it('列出记录：成败、Bot、多久以前、失败原因', async () => {
        renderHistory();
        await screen.findByText('get_group_list');
        expect(lastQuery()).toMatchObject({ limit: 100, offset: 0, ok: null, text: null, bot_id: null });
        expect(within(row('get_group_list')).getByText('3 分钟前')).toBeInTheDocument();
        expect(within(row('get_group_list')).getByText('128ms')).toBeInTheDocument();
        expect(within(row('send_group_msg')).getByText('retcode 1400')).toBeInTheDocument();
        expect(within(row('get_login_info')).getByText('等太久了，调用超时')).toBeInTheDocument();
        expect(screen.getByText('已显示全部 3 条')).toBeInTheDocument();
    });

    it('单击在新标签打开：参数写进去，当时的回包作为这个标签的结果', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const setRun = vi.spyOn(debugWorkspaceStore, 'setRun');
        renderHistory();
        fireEvent.click(await screen.findByText('get_group_list'));

        await waitFor(() => expect(open).toHaveBeenCalled());
        expect(open).toHaveBeenCalledWith('get_group_list', { newTab: true, paramsText: '{\n  "no_cache": true\n}' });
        const tabId = open.mock.results[0]?.value as string;
        expect(setRun).toHaveBeenCalledWith(tabId, {
            last: expect.objectContaining({
                botId: 'bot-sl',
                action: 'get_group_list',
                response: expect.objectContaining({
                    request_id: 'history:h1',
                    result: expect.objectContaining({ kind: 'ok' }),
                }),
            }),
        });
        expect(debugWorkspaceStore.getRun(tabId)?.last?.at).toBe(ENTRIES[0].at_ms);
    });

    it('按成败筛选；没有匹配时可以一键清除筛选', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        await user.click(screen.getByRole('radio', { name: '失败' }));
        await waitFor(() => expect(lastQuery().ok).toBe(false));
        await waitFor(() => expect(screen.queryByText('get_group_list')).not.toBeInTheDocument());
        expect(screen.getByText('send_group_msg')).toBeInTheDocument();

        service.history.mockResolvedValueOnce({ entries: [], total: 0 });
        await user.click(screen.getByRole('radio', { name: '成功' }));
        expect(await screen.findByText('没有符合条件的记录')).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '清除筛选' }));
        await waitFor(() => expect(lastQuery().ok).toBeNull());
    });

    it('搜索框按停顿后的文字查（text 字段），只看当前 Bot 带上 bot_id', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        // 搜索条默认收在标题行的图标按钮里，先展开
        act(() => revealLeftSearch('history'));
        await user.type(screen.getByRole('textbox', { name: '搜索调用历史' }), 'group');
        await waitFor(() => expect(lastQuery().text).toBe('group'));
        await user.click(screen.getByRole('button', { name: '只看当前 Bot（小雪）' }));
        await waitFor(() => expect(lastQuery().bot_id).toBe('bot-sl'));
        // 复原，筛选记在模块里
        await user.click(screen.getByRole('button', { name: '只看当前 Bot（小雪）' }));
        await user.clear(screen.getByRole('textbox', { name: '搜索调用历史' }));
        await waitFor(() => expect(lastQuery()).toMatchObject({ text: null, bot_id: null }));
    });

    it('一次取 100 条，「加载更多」再取 100 条', async () => {
        const many = Array.from({ length: 150 }, (_, i) => summary(full(`m${i}`, { action: `act_${i}` })));
        service.history.mockImplementation(async (q: DebugHistoryQuery) => ({
            entries: many.slice(q.offset, q.offset + q.limit),
            total: many.length,
        }));
        renderHistory();
        await screen.findByText('act_0');
        const list = screen.getByRole('listbox', { name: /^调用历史/ });
        // 滚到底才会画出最后一行（加载更多）
        list.scrollTop = 100 * 46;
        fireEvent.scroll(list);
        fireEvent.click(await screen.findByRole('button', { name: /加载更多（还有 50 条）/ }));
        await waitFor(() => expect(lastQuery()).toMatchObject({ limit: 200, offset: 0 }));
    });

    it('清空前确认', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        await user.click(screen.getByRole('button', { name: '清空历史' }));
        // 弹窗进场时 Radix 把背后的内容设成 aria-hidden，只按标题和按钮找
        expect(await screen.findByRole('heading', { name: '清空调用历史？' })).toBeInTheDocument();
        expect(screen.getByText(/全部 3 条记录/)).toBeInTheDocument();
        expect(service.clearHistory).not.toHaveBeenCalled();
        // Radix 模态期间 body 是 pointer-events: none，user-event 会拒绝点；直接派发点击
        fireEvent.click(screen.getByRole('button', { name: '清空' }));
        await waitFor(() => expect(service.clearHistory).toHaveBeenCalledTimes(1));
    });

    it('收藏一条：取回完整记录后弹出和中栏同一个起名框，存进收藏；已收藏过的会提醒', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('send_group_msg');
        await user.click(within(row('send_group_msg')).getByRole('button', { name: '收藏' }));
        expect(await screen.findByRole('heading', { name: '收藏请求' })).toBeInTheDocument();
        // 名字默认带上目录里的简介；当时走的通道默认不记，重发时跟着顶栏
        const name = screen.getByRole('textbox', { name: '名字' });
        expect(name).toHaveValue('发送群消息（send_group_msg）');
        expect(screen.getByRole('checkbox', { name: /记住当时走的通道/ })).not.toBeChecked();
        expect(service.saveCollections).not.toHaveBeenCalled();
        fireEvent.change(name, { target: { value: '打招呼' } });
        fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: '收藏' }));
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        const saved = service.saveCollections.mock.calls[0]?.[0] as DebugCollections;
        expect(saved.requests).toHaveLength(1);
        expect(saved.requests[0]).toMatchObject({
            name: '打招呼',
            action: 'send_group_msg',
            params: { group_id: 1, message: 'hi' },
            channel: null,
            folder_id: null,
        });
        await waitFor(() => expect(screen.queryByRole('heading', { name: '收藏请求' })).not.toBeInTheDocument());

        await user.click(within(row('send_group_msg')).getByRole('button', { name: '收藏' }));
        expect(await screen.findByText(/已经收藏过一份一样的请求：「打招呼」/)).toBeInTheDocument();
    });

    it('键盘：高亮一行后 S 收藏、C 复制参数；带 Ctrl 的不拦', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        const list = screen.getByRole('listbox', { name: /S 收藏，C 复制参数/ });
        act(() => list.focus());
        await user.keyboard('{ArrowDown}');
        await user.keyboard('c');
        await waitFor(async () => expect(await navigator.clipboard.readText()).toBe(JSON.stringify({ no_cache: true }, null, 2)));
        // 带 Ctrl 的是系统快捷键（Ctrl+S / Ctrl+C），不当成收藏 / 复制
        await user.keyboard('{Control>}s{/Control}');
        await new Promise((r) => setTimeout(r, 20));
        expect(screen.queryByRole('heading', { name: '收藏请求' })).not.toBeInTheDocument();
        await user.keyboard('s');
        expect(await screen.findByRole('heading', { name: '收藏请求' })).toBeInTheDocument();
        expect(screen.getByRole('textbox', { name: '名字' })).toHaveValue('get_group_list');
    });

    it('复制参数：写进剪贴板的是编辑器格式的 JSON', async () => {
        // userEvent.setup() 自带一个剪贴板替身，直接读回来
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        await user.click(within(row('get_group_list')).getByRole('button', { name: '复制参数' }));
        await waitFor(async () => expect(await navigator.clipboard.readText()).toBe('{\n  "no_cache": true\n}'));
    });

    it('没有记录时给出说明', async () => {
        service.history.mockResolvedValue({ entries: [], total: 0 });
        renderHistory();
        expect(await screen.findByText('还没有调用记录')).toBeInTheDocument();
    });

    it('筛选着的时候清空：写明删的是全部历史，不只是筛出来的', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        await user.click(screen.getByRole('radio', { name: '失败' }));
        await waitFor(() => expect(screen.queryByText('get_group_list')).not.toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: '清空历史' }));
        expect(await screen.findByText('会删掉全部调用历史（不只是现在筛选出的 2 条），连同当时的响应，不能撤销。收藏不受影响。')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '取消' }));
        expect(service.clearHistory).not.toHaveBeenCalled();
        // 复原，筛选记在模块里
        await waitFor(() => expect(screen.queryByRole('heading', { name: '清空调用历史？' })).not.toBeInTheDocument());
        await user.click(screen.getByRole('radio', { name: '全部' }));
        await waitFor(() => expect(lastQuery().ok).toBeNull());
    });

    it('行上右键菜单里的按键不会冒到列表上打开高亮的那一行', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findByText('get_group_list');
        const list = screen.getByRole('listbox', { name: /^调用历史/ });
        act(() => list.focus());
        await user.keyboard('{ArrowDown}');
        fireEvent.contextMenu(row('send_group_msg'));
        const item = await screen.findByRole('menuitem', { name: '复制接口名' });
        fireEvent.keyDown(item, { key: 'Enter' });
        await new Promise((r) => setTimeout(r, 20));
        expect(service.historyEntry).not.toHaveBeenCalled();
    });
});

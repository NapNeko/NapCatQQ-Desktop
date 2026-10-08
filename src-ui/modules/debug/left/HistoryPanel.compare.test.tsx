// 历史面板的「对比响应」：对比模式的开关、同接口两条的勾选门、对话框的并排 diff。

import type { ReactNode } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugHistoryEntry } from '../../../core/ipc/generated/debug/DebugHistoryEntry';
import type { DebugHistoryQuery } from '../../../core/ipc/generated/debug/DebugHistoryQuery';
import type { DebugHistorySummary } from '../../../core/ipc/generated/debug/DebugHistorySummary';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

// 虚拟列表按容器高度决定画几行：jsdom 里没有布局，行高也一并垫固定值（和 HistoryPanel.test.tsx 同一套）
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).classList.contains('overflow-y-auto') ? 600 : 30;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => 280,
    });
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
});
afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
});

const service: Record<string, ReturnType<typeof vi.fn>> = {
    history: vi.fn(),
    historyEntry: vi.fn(),
    collections: vi.fn(),
    catalog: vi.fn(),
};

vi.mock('../../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get:
                (_t, key: string) =>
                (...args: unknown[]) => {
                    const fn = service[key];
                    if (!fn) throw new Error(`没有模拟 service.${key}`);
                    return fn(...args);
                },
        },
    ),
}));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { setLeftSearchOpen } from '../leftPanels';
import { LeftColumn } from './LeftColumn';

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
        at_ms: 1_700_000_000_000,
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
        response: { status: 'ok', retcode: 0, data: { group_id: 1 }, message: '', wording: '' },
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

/** p1 较旧、p2 较新（响应里 group_id 是 2）、q1 是另一个接口 */
const OLD = full('p1', { at_ms: 1_700_000_000_000 });
const NEW = full('p2', {
    at_ms: 1_700_000_060_000,
    response: { status: 'ok', retcode: 0, data: { group_id: 2 }, message: '', wording: '' },
});
const OTHER_ACTION = full('q1', {
    action: 'send_group_msg',
    at_ms: 1_700_000_030_000,
    params: { group_id: 1, message: 'hi' },
});
const COMPARE_ENTRIES = [OLD, NEW, OTHER_ACTION];

function renderHistory() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(<LeftColumn target={SL} panel="history" />, { wrapper });
}

const compareToggle = () =>
    screen.getByRole('button', { name: '对比响应（勾选同一个接口的两条记录）' });
/** 行序固定：p1、p2、q1 */
const optionAt = (i: number) => screen.getAllByRole('option')[i]!;

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    preferencesStore.setMotionEnabled(false);
    debugWorkspaceStore._reset();
    setLeftSearchOpen('history', false);
    service.history.mockImplementation(async (q: DebugHistoryQuery) => ({
        entries: COMPARE_ENTRIES.slice(q.offset, q.offset + q.limit).map(summary),
        total: COMPARE_ENTRIES.length,
    }));
    service.historyEntry.mockImplementation(
        async (id: string) => COMPARE_ENTRIES.find((e) => e.id === id) ?? null,
    );
    service.collections.mockResolvedValue({
        version: 1,
        folders: [],
        requests: [],
    } satisfies DebugCollections);
    service.catalog.mockResolvedValue({
        backend: 'snowluma',
        source: 'live',
        snapshot_version: '0.9.0',
        actions: [],
    });
});

describe('调用历史的「对比响应」', () => {
    it('进入对比模式：单击是勾选而不是打开；第二条接口不同勾不上；退出一起清', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findAllByText('get_group_list');

        await user.click(compareToggle());
        expect(screen.getByText('勾选同一个接口的两条记录')).toBeInTheDocument();

        // 勾第一条（q1，send_group_msg）：单击没有打开新标签
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.click(optionAt(2));
        expect(optionAt(2)).toHaveAttribute('aria-selected', 'true');
        expect(screen.getByText('再勾一条同接口的')).toBeInTheDocument();
        expect(open).not.toHaveBeenCalled();

        // 第二条接口不同：勾不上，底条还是「再勾一条同接口的」
        await user.click(optionAt(0));
        expect(optionAt(0)).toHaveAttribute('aria-selected', 'false');
        expect(screen.getByText('再勾一条同接口的')).toBeInTheDocument();

        // 退出对比：勾选和底条一起没
        await user.click(screen.getByRole('button', { name: '退出对比' }));
        expect(screen.queryByText('再勾一条同接口的')).not.toBeInTheDocument();
    });

    it('勾两条同接口的记录后点「对比响应」：对话框左旧右新，改动的行高亮', async () => {
        const user = userEvent.setup();
        renderHistory();
        await screen.findAllByText('get_group_list');

        await user.click(compareToggle());
        // 先勾较新的 p2，再勾较旧的 p1：框里也得是左旧右新
        await user.click(optionAt(1));
        await user.click(optionAt(0));
        expect(screen.getByText('已选 2 条，可以对比了')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: '对比响应' }));
        expect(
            await screen.findByRole('heading', { name: /对比响应 · get_group_list/ }),
        ).toBeInTheDocument();

        // group_id 1 的那行只有旧的有（红），group_id 2 的只有新的有（绿）
        expect(await screen.findByText(/"group_id": 1/)).toHaveClass('bg-danger-soft/70');
        expect(screen.getByText(/"group_id": 2/)).toHaveClass('bg-success-soft/70');
        // 没改的行两侧各一格、都不高亮
        for (const cell of screen.getAllByText(/"status": "ok"/)) {
            expect(cell.className).not.toContain('bg-danger-soft');
            expect(cell.className).not.toContain('bg-success-soft');
        }
    });

    it('两条记录回包一致：提示一份不用翻了', async () => {
        // p2 与 p1 回包一模一样（只是时间不同）
        service.historyEntry.mockImplementation(async (id: string) =>
            id === 'p2'
                ? full('p2', { at_ms: 1_700_000_060_000 })
                : (COMPARE_ENTRIES.find((e) => e.id === id) ?? null),
        );
        const user = userEvent.setup();
        renderHistory();
        await screen.findAllByText('get_group_list');

        await user.click(compareToggle());
        await user.click(optionAt(0));
        await user.click(optionAt(1));
        await user.click(screen.getByRole('button', { name: '对比响应' }));

        expect(await screen.findByText('两份回包完全一致')).toBeInTheDocument();
    });
});

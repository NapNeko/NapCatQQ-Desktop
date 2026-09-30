import type { ReactNode } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import type { DebugCallRequest } from '../../../core/ipc/generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

const service = {
    targets: vi.fn(),
    channels: vi.fn(),
    catalog: vi.fn(),
    describe: vi.fn(),
    call: vi.fn(),
    cancel: vi.fn(),
    workspace: vi.fn(),
    saveWorkspace: vi.fn(),
    collections: vi.fn(),
};

vi.mock('../../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get: (_t, key: string) => (...args: unknown[]) =>
                (service as Record<string, (...a: unknown[]) => unknown>)[key](...args),
        },
    ),
}));
vi.mock('../../../core/services/domain-event-hub', () => ({ subscribeDomainEvents: () => () => {} }));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { debugWorkspaceStore, defaultWorkspace } from '../../../hooks/debug/debugWorkspaceStore';
import { _resetDebugCatalogForTests } from '../../../hooks/debug/useDebugCatalog';
import { CenterColumn } from './CenterColumn';
import { _resetDangerSkipsForTests } from '../DangerConfirmDialog';
import { _resetSeedStateForTests, rememberInitialText } from './seedState';

const BOT: DebugTarget = {
    bot_id: 'bot-nc',
    name: '小雪',
    qq_id: 1919810,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};

function summary(name: string, safety: DebugActionSummary['safety'], text = name): DebugActionSummary {
    return {
        name,
        aliases: [],
        summary: text,
        category: 'account',
        safety,
        stream: false,
        supported: true,
        other_backend_present: null,
        param_diff: false,
    };
}

const CATALOG: DebugActionSummary[] = [
    summary('get_group_list', 'read_only', '获取群列表'),
    summary('get_group_info', 'read_only', '获取群信息'),
    summary('get_status', 'read_only', '获取状态'),
    summary('send_group_msg', 'side_effect', '发送群消息'),
    summary('delete_msg', 'dangerous', '撤回消息'),
];

function spec(name: string, patch: Partial<DebugActionSpec> = {}): DebugActionSpec {
    const s = CATALOG.find((a) => a.name === name);
    return {
        name,
        aliases: [],
        summary: s?.summary ?? name,
        description: null,
        category: 'account',
        safety: s?.safety ?? 'read_only',
        stream: false,
        supported: true,
        params_schema: { type: 'object', properties: {} },
        returns_schema: null,
        returns_text: null,
        return_example: null,
        examples: [],
        error_examples: [],
        invariants: [],
        other_backend: null,
        source: 'live',
        ...patch,
    };
}

const SPECS: Record<string, DebugActionSpec> = Object.fromEntries(
    CATALOG.map((a) => [a.name, spec(a.name)]),
);

function tab(id: string, action: string, params_text = '{}'): DebugRequestDraft {
    return { id, action, params_text, timeout_ms: null, channel: null };
}

function okResponse(req: DebugCallRequest): DebugCallResponse {
    return {
        request_id: req.request_id,
        result: {
            kind: 'ok',
            outcome: {
                ok: true,
                status: 'ok',
                retcode: 0,
                data: {},
                message: '',
                wording: '',
                raw: {},
                elapsed_ms: 3,
                channel: { kind: 'internal' },
                size_bytes: 2,
                truncated: false,
            },
        },
    };
}

async function renderColumn(tabs: DebugRequestDraft[], active = tabs[0]?.id ?? null) {
    service.workspace.mockResolvedValue({ ...defaultWorkspace(), tabs, active_tab: active, selected_bot: BOT.bot_id });
    await debugWorkspaceStore.load();
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(
        <CenterColumn target={BOT} callChannel={{ kind: 'auto' }} onOpenPalette={vi.fn()} onRevealCallChannel={vi.fn()} />,
        { wrapper },
    );
}

const actionInput = () => screen.getByRole('combobox', { name: '接口名' }) as HTMLInputElement;
const activeTab = () => debugWorkspaceStore.getSnapshot().ws.tabs.find((t) => t.id === debugWorkspaceStore.getSnapshot().ws.active_tab);
const editorCalls = () => service.call.mock.calls.map((c) => c[0] as DebugCallRequest).filter((r) => r.origin === 'editor');

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
    Range.prototype.getClientRects ??= () => ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] }) as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect ??= () => new DOMRect();
});

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    debugWorkspaceStore._reset();
    _resetDebugCatalogForTests();
    _resetDangerSkipsForTests();
    _resetSeedStateForTests();
    preferencesStore.setMotionEnabled(false);
    service.targets.mockResolvedValue([BOT]);
    service.saveWorkspace.mockResolvedValue(undefined);
    service.channels.mockResolvedValue({
        bot_id: BOT.bot_id,
        channels: [],
        auto_call: { kind: 'internal' },
        auto_events: { kind: 'internal' },
    });
    service.catalog.mockResolvedValue({ backend: 'napcat', source: 'live', snapshot_version: 't', actions: CATALOG });
    service.describe.mockImplementation(async (_b: string, _k: string, name: string) => SPECS[name] ?? null);
    service.collections.mockResolvedValue({ version: 1, folders: [], requests: [] });
    service.cancel.mockResolvedValue(undefined);
    service.call.mockImplementation(async (req: DebugCallRequest) => okResponse(req));
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('接口名输入框', () => {
    it('回车挑建议：只提交一次，提交的是挑中的那个而不是敲了一半的字', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', '')]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.click(actionInput());
        await user.type(actionInput(), 'get_group_l');
        expect(await screen.findByRole('option', { name: /get_group_list/ })).toBeInTheDocument();
        await user.keyboard('{Enter}');

        expect(open).toHaveBeenCalledTimes(1);
        expect(open).toHaveBeenCalledWith('get_group_list', {});
        expect(activeTab()?.action).toBe('get_group_list');
        await waitFor(() => expect(actionInput().value).toBe('get_group_list'));
    });

    it('点建议：只提交一次，焦点留在输入框', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', '')]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.click(actionInput());
        await user.type(actionInput(), 'get_gr');
        await user.click(await screen.findByRole('option', { name: /get_group_info/ }));

        expect(open).toHaveBeenCalledTimes(1);
        expect(open).toHaveBeenCalledWith('get_group_info', {});
        expect(activeTab()?.action).toBe('get_group_info');
        expect(document.activeElement).toBe(actionInput());
    });

    it('失焦等于放弃：敲了一半的名字不会被当成接口名', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status')]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.click(actionInput());
        await user.keyboard('{Backspace}{Backspace}');
        expect(actionInput().value).toBe('get_stat');
        await user.click(screen.getByRole('radio', { name: 'JSON' }));

        expect(open).not.toHaveBeenCalled();
        expect(activeTab()?.action).toBe('get_status');
        expect(actionInput().value).toBe('get_status');
    });

    it('点输入框本身不会把建议列表收起', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', '')]);
        await user.click(actionInput());
        await user.type(actionInput(), 'get_');
        expect(await screen.findByRole('listbox', { name: '接口建议' })).toBeVisible();
        await user.click(actionInput());
        expect(screen.getByRole('listbox', { name: '接口建议' })).toBeVisible();
        expect(actionInput()).toHaveAttribute('aria-expanded', 'true');
    });

    it('草稿没提交时 Ctrl+Enter 只换接口不发送；再按一次才用新接口发', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status')]);
        await waitFor(() => expect(screen.getByRole('button', { name: /^发送/ })).toBeEnabled());
        await user.tripleClick(actionInput());
        await user.keyboard('get_group_list');
        await user.keyboard('{Control>}{Enter}{/Control}');

        expect(activeTab()?.action).toBe('get_group_list');
        expect(editorCalls()).toHaveLength(0);

        await waitFor(() => expect(screen.getByRole('button', { name: /^发送/ })).toBeEnabled());
        await user.keyboard('{Control>}{Enter}{/Control}');
        await waitFor(() => expect(editorCalls()).toHaveLength(1));
        expect(editorCalls()[0]!.action).toBe('get_group_list');
    });

    it('没在改名字时 Esc 放行给「取消调用」；改名字时 Esc 只放弃草稿', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(
            (req: DebugCallRequest) =>
                new Promise<DebugCallResponse>((resolve) => {
                    if (req.origin !== 'editor') resolve(okResponse(req));
                }),
        );
        await renderColumn([tab('t1', 'get_status')]);
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);
        await screen.findByRole('button', { name: /取消/ });

        // 改名字：Esc 只是放弃，不取消调用
        await user.click(actionInput());
        await user.keyboard('x');
        await user.keyboard('{Escape}');
        expect(actionInput().value).toBe('get_status');
        expect(service.cancel).not.toHaveBeenCalled();

        // 没在改：Esc 交给页面，取消进行中的调用
        await user.keyboard('{Escape}');
        await waitFor(() => expect(service.cancel).toHaveBeenCalledTimes(1));
    });
});

describe('目录外的变体按原接口分级', () => {
    it('delete_msg_async 按 delete_msg 算危险：显示分级来源，发送前确认，后果按原接口写', async () => {
        const user = userEvent.setup({ pointerEventsCheck: 0 });
        await renderColumn([tab('t1', 'delete_msg_async', '{\n  "message_id": 5\n}')]);
        expect(await screen.findByText('按 delete_msg 分级')).toBeInTheDocument();
        expect(screen.getByText('危险')).toBeInTheDocument();

        const send = screen.getByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);
        expect(await screen.findByText('会撤回消息 5')).toBeInTheDocument();
        expect(editorCalls()).toHaveLength(0);
    });

    it('send_group_msg_async 按有副作用处理，不弹确认；完全不认识的名字也按有副作用处理', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'send_group_msg_async', '{}'), tab('t2', 'totally_unknown', '{}')]);
        expect(await screen.findByText('按 send_group_msg 分级')).toBeInTheDocument();
        expect(screen.getByText('有副作用')).toBeInTheDocument();
        await waitFor(() => expect(screen.getByRole('button', { name: /^发送/ })).toBeEnabled());
        await user.click(screen.getByRole('button', { name: /^发送/ }));
        await waitFor(() => expect(editorCalls()).toHaveLength(1));

        await user.click(screen.getByRole('tab', { name: /totally_unknown/ }));
        expect(await screen.findByText('目录里没有')).toBeInTheDocument();
        await waitFor(() => expect(screen.getByRole('button', { name: /^发送/ })).toBeEnabled());
        await user.click(screen.getByRole('button', { name: /^发送/ }));
        await waitFor(() => expect(editorCalls()).toHaveLength(2));
    });
});

describe('标签条', () => {
    it('Delete 关掉聚焦的标签后，焦点落到接替它的标签上', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status'), tab('t2', 'get_group_list')], 't1');
        const first = screen.getByRole('tab', { name: /get_status/ });
        first.focus();
        await user.keyboard('{Delete}');

        const tabs = within(screen.getByRole('tablist', { name: '请求标签' })).getAllByRole('tab');
        expect(tabs).toHaveLength(1);
        await waitFor(() => expect(document.activeElement).toBe(tabs[0]));
        expect(tabs[0]).toHaveTextContent('get_group_list');
    });

    it('关掉最后一个标签时焦点落到「新请求」按钮上', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status')]);
        screen.getByRole('tab', { name: /get_status/ }).focus();
        await user.keyboard('{Delete}');
        await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('button', { name: '新请求' })));
    });
});

describe('修复第 2 轮', () => {
    it('没敲字、用 ↓ 打开列表再选：回车提交高亮的那个', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status')]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.click(actionInput());
        await user.keyboard('{ArrowDown}');
        expect(await screen.findByRole('listbox', { name: '接口建议' })).toBeVisible();
        // 空查询按名字排：delete_msg、get_group_info、get_group_list……
        await user.keyboard('{ArrowDown}');
        await user.keyboard('{Enter}');

        expect(open).toHaveBeenCalledTimes(1);
        expect(open).toHaveBeenCalledWith('get_group_info', {});
        expect(activeTab()?.action).toBe('get_group_info');
    });

    it('改过参数的标签改接口名：先问一句；选「新开标签」时另开的标签带着参数，而且算改过，之后从目录点接口不会把它顶掉', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status', '{\n  "x": 1\n}')]);
        await user.tripleClick(actionInput());
        await user.keyboard('get_group_list');
        // 不从列表挑，直接提交敲的字
        await user.keyboard('{Control>}{Enter}{/Control}');

        const ws = () => debugWorkspaceStore.getSnapshot().ws;
        const choice = screen.getByRole('group', { name: /参数改过了，换成 get_group_list/ });
        expect(within(choice).getByRole('button', { name: '替换当前标签' })).toHaveFocus();
        expect(ws().tabs).toHaveLength(1);
        await user.click(within(choice).getByRole('button', { name: '新开标签' }));

        expect(ws().tabs).toHaveLength(2);
        const renamed = ws().tabs.find((t) => t.id === ws().active_tab)!;
        expect(renamed.action).toBe('get_group_list');
        expect(JSON.parse(renamed.params_text)).toEqual({ x: 1 });
        // 原来的标签不动
        expect(ws().tabs.find((t) => t.id === 't1')?.action).toBe('get_status');

        // 说明读到后也不会被当成空标签重新填
        await waitFor(() => expect(actionInput()).toHaveValue('get_group_list'));
        expect(JSON.parse(ws().tabs.find((t) => t.id === renamed.id)!.params_text)).toEqual({ x: 1 });

        act(() => {
            debugWorkspaceStore.openAction('delete_msg');
        });
        expect(ws().tabs).toHaveLength(3);
        expect(ws().tabs.find((t) => t.id === renamed.id)?.action).toBe('get_group_list');
    });

    it('改过参数的标签选「替换当前标签」：原地换接口，参数、通道留着，旧结果丢掉；之后从目录点接口另开', async () => {
        const user = userEvent.setup();
        await renderColumn([{ ...tab('t1', 'get_group_info', '{\n  "group_id": 7\n}'), channel: { kind: 'internal' } }]);
        debugWorkspaceStore.setRun('t1', {
            last: { response: okResponse({ request_id: 'r0' } as DebugCallRequest), at: 1, botId: BOT.bot_id, action: 'get_group_info' },
        });
        await user.tripleClick(actionInput());
        await user.keyboard('get_group_list{Enter}');
        await user.click(screen.getByRole('button', { name: '替换当前标签' }));

        const ws = () => debugWorkspaceStore.getSnapshot().ws;
        expect(ws().tabs).toEqual([
            { id: 't1', action: 'get_group_list', params_text: '{\n  "group_id": 7\n}', timeout_ms: null, channel: { kind: 'internal' } },
        ]);
        expect(debugWorkspaceStore.getRun('t1')).toBeUndefined();
        expect(screen.queryByRole('group', { name: /参数改过了/ })).not.toBeInTheDocument();
        await waitFor(() => expect(actionInput()).toHaveFocus());
        // 说明读到后参数也不会被重新填
        await waitFor(() => expect(service.describe).toHaveBeenCalledWith(BOT.bot_id, BOT.backend, 'get_group_list'));
        expect(ws().tabs[0].params_text).toBe('{\n  "group_id": 7\n}');
        act(() => {
            debugWorkspaceStore.openAction('get_status');
        });
        expect(ws().tabs).toHaveLength(2);
    });

    it('换接口的那一问按 Esc 取消：什么都不动，Esc 也不会去取消进行中的调用', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_group_info', '{\n  "group_id": 7\n}')]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.tripleClick(actionInput());
        await user.keyboard('get_group_list{Enter}');
        expect(screen.getByRole('button', { name: '替换当前标签' })).toHaveFocus();
        await user.keyboard('{Escape}');
        expect(screen.queryByRole('group', { name: /参数改过了/ })).not.toBeInTheDocument();
        expect(open).not.toHaveBeenCalled();
        expect(activeTab()?.action).toBe('get_group_info');
        expect(service.cancel).not.toHaveBeenCalled();
        await waitFor(() => expect(actionInput()).toHaveFocus());
        expect(actionInput()).toHaveValue('get_group_info');
    });

    it('用 ↓ 重新打开建议列表时高亮回到第一项', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status')]);
        await user.click(actionInput());
        await user.keyboard('{ArrowDown}');
        await screen.findByRole('listbox', { name: '接口建议' });
        await user.keyboard('{ArrowDown}{ArrowDown}');
        expect(screen.getAllByRole('option')[2]).toHaveAttribute('aria-selected', 'true');
        await user.keyboard('{Escape}');
        await waitFor(() => expect(screen.queryByRole('listbox', { name: '接口建议' })).not.toBeInTheDocument());
        await user.keyboard('{ArrowDown}');
        await screen.findByRole('listbox', { name: '接口建议' });
        expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');
    });

    it('说明还在读时判断「改没改过」用记下的初始参数：和初始一样就原地换接口', async () => {
        const user = userEvent.setup();
        service.describe.mockImplementation(() => new Promise<DebugActionSpec>(() => {}));
        const initial = '{\n  "group_id": 0\n}';
        rememberInitialText('t1', initial);
        await renderColumn([tab('t1', 'get_group_info', initial)]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.tripleClick(actionInput());
        await user.keyboard('get_status');
        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(open).toHaveBeenCalledWith('get_status', {});
    });

    it('说明还在读、又没记过初始参数时，按「不空就算改过」另开标签带走参数', async () => {
        const user = userEvent.setup();
        service.describe.mockImplementation(() => new Promise<DebugActionSpec>(() => {}));
        await renderColumn([tab('t1', 'get_group_info', '{\n  "group_id": 7\n}')]);
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.tripleClick(actionInput());
        await user.keyboard('get_status');
        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(open).not.toHaveBeenCalled();
        await user.click(screen.getByRole('button', { name: '新开标签' }));
        expect(open).toHaveBeenCalledWith('get_status', { newTab: true });
        const ws = debugWorkspaceStore.getSnapshot().ws;
        expect(JSON.parse(ws.tabs.find((t) => t.id === ws.active_tab)!.params_text)).toEqual({ group_id: 7 });
    });

    it('Ctrl+W 这类在别处关掉当前标签时，掉了的焦点交给接替的标签；焦点在别处时不抢', async () => {
        const user = userEvent.setup();
        await renderColumn([tab('t1', 'get_status'), tab('t2', 'get_group_list'), tab('t3', 'send_group_msg')], 't1');
        // 焦点在当前标签里面（接口名输入框），页面的 Ctrl+W 调的就是 closeTab
        await user.click(actionInput());
        act(() => debugWorkspaceStore.closeTab('t1'));
        await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('tab', { name: /get_group_list/ })));

        // 焦点在中栏外面：关标签不抢焦点
        const outside = document.createElement('button');
        document.body.appendChild(outside);
        outside.focus();
        act(() => debugWorkspaceStore.closeTab('t2'));
        await waitFor(() => expect(within(screen.getByRole('tablist', { name: '请求标签' })).getAllByRole('tab')).toHaveLength(1));
        expect(document.activeElement).toBe(outside);
        outside.remove();
    });
});

describe('换 Bot 后「改没改过」（M2：只拿填进去的那份初始参数比，不按新说明重算）', () => {
    const SL_BOT: DebugTarget = { ...BOT, bot_id: 'bot-sl', name: 'SL 号', backend: 'snowluma' };
    // 同一个接口 NC 按示例填 "" 占位，SL 没有示例、是 {}：两边初始参数不一样
    const NC_INITIAL = '{\n  "group_id": ""\n}';

    function mockTwoBackends() {
        service.describe.mockImplementation(async (botId: string, _backend: string, name: string) => {
            if (name === 'get_group_info') {
                return botId === SL_BOT.bot_id ? spec('get_group_info', { summary: 'SL 取群信息' }) : spec('get_group_info', { examples: [{ group_id: '' }] });
            }
            return SPECS[name] ?? null;
        });
    }

    async function switchTo(view: ReturnType<typeof renderColumn> extends Promise<infer R> ? R : never, target: DebugTarget) {
        view.rerender(
            <CenterColumn target={target} callChannel={{ kind: 'auto' }} onOpenPalette={vi.fn()} onRevealCallChannel={vi.fn()} />,
        );
        // 「SL 取群信息」只在 SL 的说明真的渲染出来后才会出现，此时「改没改过」的判断一定已经按新数据跑过
        await screen.findByText(target === SL_BOT ? 'SL 取群信息' : '获取群信息');
    }

    it('没动过的标签切到初始参数不同的 Bot：不亮小点，改接口名不被追问，从目录点接口直接顶替', async () => {
        const user = userEvent.setup();
        mockTwoBackends();
        const view = await renderColumn([tab('t1', 'get_group_info')]);
        // NC 的说明读到后按示例填上
        await waitFor(() => expect(activeTab()?.params_text).toBe(NC_INITIAL));
        expect(screen.queryByLabelText('参数改过')).not.toBeInTheDocument();

        await switchTo(view, SL_BOT);
        expect(screen.queryByLabelText('参数改过')).not.toBeInTheDocument();

        // 改接口名不问「替换 / 新开」，原地换
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.tripleClick(actionInput());
        await user.keyboard('get_status');
        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(open).toHaveBeenCalledWith('get_status', {});
        expect(screen.queryByRole('group', { name: /参数改过了/ })).not.toBeInTheDocument();
        await waitFor(() => expect(activeTab()?.action).toBe('get_status'));

        // 从目录点接口也一样：没动过就直接顶替，不另开标签
        act(() => {
            debugWorkspaceStore.openAction('get_group_list');
        });
        const ws = debugWorkspaceStore.getSnapshot().ws;
        expect(ws.tabs).toHaveLength(1);
        expect(ws.tabs[0]).toMatchObject({ id: 't1', action: 'get_group_list', params_text: '{}' });
    });

    it('真改过参数的标签：切到另一边照样亮小点，改接口名照样问', async () => {
        const user = userEvent.setup();
        mockTwoBackends();
        const view = await renderColumn([tab('t1', 'get_group_info')]);
        await waitFor(() => expect(activeTab()?.params_text).toBe(NC_INITIAL));
        act(() => debugWorkspaceStore.setParamsText('t1', '{\n  "group_id": "4321"\n}'));
        expect(screen.getByLabelText('参数改过')).toBeInTheDocument();

        await switchTo(view, SL_BOT);
        expect(screen.getByLabelText('参数改过')).toBeInTheDocument();

        await user.tripleClick(actionInput());
        await user.keyboard('get_status');
        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(await screen.findByRole('group', { name: /参数改过了，换成 get_status/ })).toBeInTheDocument();
    });

    it('换回同一个 Bot、路由切走再回来：行为和原来一样，还是不亮小点', async () => {
        mockTwoBackends();
        const view = await renderColumn([tab('t1', 'get_group_info')]);
        await waitFor(() => expect(activeTab()?.params_text).toBe(NC_INITIAL));

        // NC → SL → NC 来回切
        await switchTo(view, SL_BOT);
        await switchTo(view, BOT);
        expect(screen.queryByLabelText('参数改过')).not.toBeInTheDocument();
        expect(activeTab()?.params_text).toBe(NC_INITIAL);

        // 路由切走再回来（中栏重挂）：「填过初始参数」的记忆在模块里，依旧不亮、参数不被重填
        view.unmount();
        await renderColumn([tab('t1', 'get_group_info', NC_INITIAL)]);
        await screen.findByText('获取群信息');
        expect(screen.queryByLabelText('参数改过')).not.toBeInTheDocument();
        expect(activeTab()?.params_text).toBe(NC_INITIAL);
    });
});

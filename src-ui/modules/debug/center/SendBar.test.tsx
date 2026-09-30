import type { ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugCallRequest } from '../../../core/ipc/generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { DebugWorkspace } from '../../../core/ipc/generated/debug/DebugWorkspace';

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
    history: vi.fn(),
    saveResponseFile: vi.fn(),
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
import { _resetSeedStateForTests } from './seedState';

const BOT: DebugTarget = {
    bot_id: 'bot-nc',
    name: '小雪',
    qq_id: 1919810,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};

function spec(name: string, patch: Partial<DebugActionSpec>): DebugActionSpec {
    return {
        name,
        aliases: [],
        summary: name,
        description: null,
        category: 'account',
        safety: 'read_only',
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

const SPECS: Record<string, DebugActionSpec> = {
    delete_msg: spec('delete_msg', {
        safety: 'dangerous',
        category: 'message',
        params_schema: {
            type: 'object',
            properties: { message_id: { type: 'integer', 'x-ncd-role': 'message_id' } },
            required: ['message_id'],
        },
    }),
    get_login_info: spec('get_login_info', {}),
    get_stranger_info: spec('get_stranger_info', {
        category: 'friend',
        params_schema: {
            type: 'object',
            properties: { user_id: { type: 'integer', 'x-ncd-role': 'user_id' } },
            required: ['user_id'],
        },
    }),
    get_status: spec('get_status', {}),
};

function okResponse(req: DebugCallRequest, data: unknown): DebugCallResponse {
    return {
        request_id: req.request_id,
        result: {
            kind: 'ok',
            outcome: {
                ok: true,
                status: 'ok',
                retcode: 0,
                data,
                message: '',
                wording: '',
                raw: { status: 'ok', retcode: 0, data },
                elapsed_ms: 12,
                channel: { kind: 'internal' },
                size_bytes: 40,
                truncated: false,
            },
        },
    };
}

function workspace(action: string, params_text: string): DebugWorkspace {
    return {
        ...defaultWorkspace(),
        tabs: [{ id: 't1', action, params_text, timeout_ms: null, channel: null }],
        active_tab: 't1',
        selected_bot: BOT.bot_id,
    };
}

async function renderColumn(action: string, params_text: string) {
    service.workspace.mockResolvedValue(workspace(action, params_text));
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

function editorCalls(): DebugCallRequest[] {
    return service.call.mock.calls.map((c) => c[0] as DebugCallRequest).filter((r) => r.origin === 'editor');
}

// jsdom 里元素尺寸都是 0，虚拟列表一行都不画：给 JSON 树一个 440px 的视口
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
    Range.prototype.getClientRects ??= () => ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] }) as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect ??= () => new DOMRect();
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).getAttribute('role') === 'tree' ? 440 : 22;
        },
    });
});
afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
});

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    debugWorkspaceStore._reset();
    _resetDebugCatalogForTests();
    _resetSeedStateForTests();
    _resetDangerSkipsForTests();
    preferencesStore.setMotionEnabled(false);
    service.targets.mockResolvedValue([BOT]);
    service.saveWorkspace.mockResolvedValue(undefined);
    service.channels.mockResolvedValue({
        bot_id: BOT.bot_id,
        channels: [],
        auto_call: { kind: 'internal' },
        auto_events: { kind: 'internal' },
    });
    service.catalog.mockResolvedValue({ backend: 'napcat', source: 'live', snapshot_version: 't', actions: [] });
    service.describe.mockImplementation(async (_b: string, _k: string, name: string) => SPECS[name] ?? null);
    service.collections.mockResolvedValue({ version: 1, folders: [], requests: [] });
    service.cancel.mockResolvedValue(undefined);
    service.call.mockImplementation(async (req: DebugCallRequest) => okResponse(req, []));
});

afterEach(() => {
    cleanup();
});

describe('危险接口的确认', () => {
    it('发送前弹确认框写明后果；勾了「本次不再询问」之后同一个 Bot 上的这个接口不再问', async () => {
        // 模态对话框打开时 Radix 给 body 设了 pointer-events: none，内容上的 pointer-events-auto 是 Tailwind 类，jsdom 不认
        const user = userEvent.setup({ pointerEventsCheck: 0 });
        await renderColumn('delete_msg', '{\n  "message_id": 42\n}');

        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);

        // 共享 Dialog 的内容节点上没有 role，按标题和文案找
        expect(await screen.findByText('会撤回消息 42')).toBeInTheDocument();
        expect(screen.getByRole('heading', { name: /确认调用 delete_msg/ })).toBeInTheDocument();
        expect(editorCalls()).toHaveLength(0);
        // 对话框开着时 Ctrl+Enter 不会绕过确认直接发
        fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true });
        expect(editorCalls()).toHaveLength(0);

        // 取消：什么都不发
        await user.click(screen.getByRole('button', { name: '取消' }));
        await waitFor(() => expect(screen.queryByText('会撤回消息 42')).not.toBeInTheDocument());
        expect(editorCalls()).toHaveLength(0);

        // 再发一次，这回确认，并勾上「本次不再询问」
        await user.click(screen.getByRole('button', { name: /^发送/ }));
        await screen.findByText('会撤回消息 42');
        await user.click(screen.getByRole('checkbox'));
        await user.click(screen.getByRole('button', { name: '确认调用' }));
        await waitFor(() => expect(editorCalls()).toHaveLength(1));
        expect(editorCalls()[0]).toMatchObject({
            bot_id: BOT.bot_id,
            action: 'delete_msg',
            params: { message_id: 42 },
            channel: { kind: 'auto' },
            origin: 'editor',
        });

        // 第三次直接发，不再弹框
        await waitFor(() => expect(screen.getByRole('button', { name: /^发送/ })).toBeEnabled());
        await user.click(screen.getByRole('button', { name: /^发送/ }));
        await waitFor(() => expect(editorCalls()).toHaveLength(2));
        expect(screen.queryByText('会撤回消息 42')).not.toBeInTheDocument();
    });
});

describe('连发只显示最后一次', () => {
    it('第一次的回包比第二次晚到时不覆盖第二次的结果', async () => {
        const user = userEvent.setup();
        const pending: Array<(data: unknown) => void> = [];
        service.call.mockImplementation(
            (req: DebugCallRequest) =>
                new Promise<DebugCallResponse>((resolve) => {
                    if (req.origin !== 'editor') {
                        resolve(okResponse(req, []));
                        return;
                    }
                    pending.push((data) => resolve(okResponse(req, data)));
                }),
        );
        await renderColumn('get_login_info', '{}');

        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);
        // 发送中：按钮换成取消，显示「取消只是不再等」
        expect(await screen.findByRole('button', { name: /取消/ })).toBeInTheDocument();
        expect(screen.getByText(/取消只是不再等回包/)).toBeInTheDocument();

        // 还在等的时候再按一次 Ctrl+Enter
        fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true });
        await waitFor(() => expect(pending).toHaveLength(2));

        await act(async () => {
            pending[1]!({ nickname: 'second' });
        });
        expect(await screen.findByText('"second"')).toBeInTheDocument();

        await act(async () => {
            pending[0]!({ nickname: 'first' });
        });
        expect(screen.queryByText('"first"')).not.toBeInTheDocument();
        expect(screen.getByText('"second"')).toBeInTheDocument();
        expect(screen.getByText('成功')).toBeInTheDocument();
    });

    it('等回包时按 Esc 取消；没在等时 Esc 不归这一栏', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(
            (req: DebugCallRequest) =>
                new Promise<DebugCallResponse>((resolve) => {
                    if (req.origin !== 'editor') resolve(okResponse(req, []));
                    // 编辑器发的这次一直不回，等取消
                }),
        );
        await renderColumn('get_login_info', '{}');
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());

        const idleEsc = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
        window.dispatchEvent(idleEsc);
        expect(idleEsc.defaultPrevented).toBe(false);

        await user.click(send);
        await screen.findByRole('button', { name: /取消/ });
        fireEvent.keyDown(window, { key: 'Escape' });
        await waitFor(() => expect(service.cancel).toHaveBeenCalledTimes(1));
    });
});

describe('不能发的时候写明原因', () => {
    it('Bot 没在运行时按钮禁用并写「Bot 没在运行」', async () => {
        service.workspace.mockResolvedValue(workspace('get_login_info', '{}'));
        await debugWorkspaceStore.load();
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        render(
            <QueryClientProvider client={client}>
                <TooltipProvider>
                    <CenterColumn
                        target={{ ...BOT, running: false }}
                        callChannel={{ kind: 'auto' }}
                        onOpenPalette={vi.fn()}
                        onRevealCallChannel={vi.fn()}
                    />
                </TooltipProvider>
            </QueryClientProvider>,
        );
        expect(await screen.findByText('Bot 没在运行')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /^发送/ })).toBeDisabled();
    });
});

describe('确认框和新开的标签', () => {
    it('确认框开着时 Bot 停了：点「确认调用」也不发，按钮旁写明原因', async () => {
        const user = userEvent.setup({ pointerEventsCheck: 0 });
        service.workspace.mockResolvedValue(workspace('delete_msg', '{\n  "message_id": 42\n}'));
        await debugWorkspaceStore.load();
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        const ui = (target: DebugTarget) => (
            <QueryClientProvider client={client}>
                <TooltipProvider>
                    <CenterColumn target={target} callChannel={{ kind: 'auto' }} onOpenPalette={vi.fn()} onRevealCallChannel={vi.fn()} />
                </TooltipProvider>
            </QueryClientProvider>
        );
        const { rerender } = render(ui(BOT));
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);
        await screen.findByText('会撤回消息 42');

        rerender(ui({ ...BOT, running: false }));
        await user.click(screen.getByRole('button', { name: '确认调用' }));
        await waitFor(() => expect(screen.queryByText('会撤回消息 42')).not.toBeInTheDocument());
        expect(editorCalls()).toHaveLength(0);
        expect(screen.getByText('Bot 没在运行')).toBeInTheDocument();
    });

    it('「用它新开」打开的标签算改过的：带着参数，之后从目录点别的接口会另开而不是顶掉它', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(async (req: DebugCallRequest) => okResponse(req, { user_id: 10001, nickname: 'x' }));
        await renderColumn('get_login_info', '{}');
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);

        const row = await waitFor(() => {
            const r = screen.getAllByRole('treeitem').find((el) => el.textContent?.startsWith('user_id'));
            if (!r) throw new Error('回包还没画出来');
            return r;
        });
        await user.click(row.querySelector('.cursor-pointer') as HTMLElement);
        await user.click(await screen.findByRole('button', { name: /用它新开/ }));

        const ws = () => debugWorkspaceStore.getSnapshot().ws;
        expect(ws().tabs).toHaveLength(2);
        const opened = ws().tabs.find((t) => t.id === ws().active_tab)!;
        expect(opened.action).toBe('get_stranger_info');
        expect(JSON.parse(opened.params_text)).toEqual({ user_id: 10001 });
        // 说明读到后不会被当成空标签重新填
        await waitFor(() => expect(screen.getByRole('combobox', { name: '接口名' })).toHaveValue('get_stranger_info'));
        expect(JSON.parse(ws().tabs.find((t) => t.id === opened.id)!.params_text)).toEqual({ user_id: 10001 });
        // 标签上带「改过」的点
        expect(await screen.findByLabelText('参数改过')).toBeInTheDocument();

        act(() => {
            debugWorkspaceStore.openAction('get_status');
        });
        expect(ws().tabs).toHaveLength(3);
        expect(ws().tabs.find((t) => t.id === opened.id)?.action).toBe('get_stranger_info');
    });

    it('发送时给读屏播报一句「正在等回包」，计时本身不是 live region', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(
            (req: DebugCallRequest) =>
                new Promise<DebugCallResponse>((resolve) => {
                    if (req.origin !== 'editor') resolve(okResponse(req, []));
                }),
        );
        await renderColumn('get_login_info', '{}');
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);
        expect(await screen.findByText('已发送，正在等回包')).toBeInTheDocument();
        const counter = screen.getByText(/\d+\.\d 秒/);
        expect(counter.closest('[aria-live]')).toBeNull();
    });
});

describe('读屏播报', () => {
    it('第一次拿到结果也会播报：播报用的节点在发送之前就已经在了', async () => {
        const user = userEvent.setup();
        const { container } = await renderColumn('get_login_info', '{}');
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        const before = [...container.querySelectorAll('span.sr-only[role="status"]')];
        expect(before.every((el) => !el.textContent?.includes('retcode'))).toBe(true);

        await user.click(send);
        await waitFor(() => {
            const region = [...container.querySelectorAll('span.sr-only[role="status"]')].find((el) =>
                el.textContent?.includes('成功，retcode 0'),
            );
            expect(region).toBeDefined();
            expect(before).toContain(region);
        });
    });

    it('发不了的时候按 Ctrl+Enter，播报原因', async () => {
        service.workspace.mockResolvedValue(workspace('get_login_info', '{}'));
        await debugWorkspaceStore.load();
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        render(
            <QueryClientProvider client={client}>
                <TooltipProvider>
                    <CenterColumn
                        target={{ ...BOT, running: false }}
                        callChannel={{ kind: 'auto' }}
                        onOpenPalette={vi.fn()}
                        onRevealCallChannel={vi.fn()}
                    />
                </TooltipProvider>
            </QueryClientProvider>,
        );
        await screen.findByText('Bot 没在运行');
        expect(screen.queryByText(/发不了：/)).not.toBeInTheDocument();
        fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true });
        const status = await screen.findByText(/发不了：Bot 没在运行/);
        expect(status).toHaveAttribute('role', 'status');
        expect(editorCalls()).toHaveLength(0);
    });

    it('播报的是按下那一刻的原因：之后原因变了不跟着再读，再按一次才读新的', async () => {
        service.workspace.mockResolvedValue(workspace('get_login_info', '{}'));
        await debugWorkspaceStore.load();
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        const ui = (running: boolean) => (
            <QueryClientProvider client={client}>
                <TooltipProvider>
                    <CenterColumn
                        target={{ ...BOT, running }}
                        callChannel={{ kind: 'auto' }}
                        onOpenPalette={vi.fn()}
                        onRevealCallChannel={vi.fn()}
                    />
                </TooltipProvider>
            </QueryClientProvider>
        );
        const { container, rerender } = render(ui(true));
        const regions = () => [...container.querySelectorAll('span.sr-only[role="status"]')].map((el) => el.textContent ?? '');
        const tabId = () => debugWorkspaceStore.getSnapshot().ws.active_tab!;
        await screen.findByRole('button', { name: /^发送/ });
        act(() => debugWorkspaceStore.setParamsText(tabId(), '{,'));
        await screen.findByText('JSON 有错，改好再发');
        fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true });
        await waitFor(() => expect(regions().some((t) => t.startsWith('发不了：JSON 有错'))).toBe(true));

        // 改好了、Bot 又停了：原因变了，但没有再按，播报区不该冒出新的一句
        act(() => debugWorkspaceStore.setParamsText(tabId(), '{}'));
        rerender(ui(false));
        await screen.findByText('Bot 没在运行');
        await new Promise((r) => setTimeout(r, 20));
        expect(regions().some((t) => t.includes('发不了'))).toBe(false);

        fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true });
        await waitFor(() => expect(regions().some((t) => t.startsWith('发不了：Bot 没在运行'))).toBe(true));
    });
});

describe('超大回包', () => {
    it('被截断的回包给「另存完整内容」，按这次调用的请求 id 另存', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(async (req: DebugCallRequest) => {
            const res = okResponse(req, null);
            if (res.result.kind === 'ok') {
                res.result.outcome = { ...res.result.outcome, truncated: true, raw: '{"status":"ok","data":[', size_bytes: 6 * 1024 * 1024 };
            }
            return res;
        });
        service.saveResponseFile.mockResolvedValue(true);
        await renderColumn('get_login_info', '{}');
        const send = await screen.findByRole('button', { name: /^发送/ });
        await waitFor(() => expect(send).toBeEnabled());
        await user.click(send);
        expect(await screen.findByText(/回包有 6 MB，太大了/)).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '另存完整内容' }));
        const requestId = (service.call.mock.calls[0]?.[0] as DebugCallRequest).request_id;
        await waitFor(() => expect(service.saveResponseFile).toHaveBeenCalledTimes(1));
        expect(service.saveResponseFile).toHaveBeenCalledWith(requestId, expect.stringMatching(/^get_login_info-\d{8}-\d{6}\.json$/));
    });
});

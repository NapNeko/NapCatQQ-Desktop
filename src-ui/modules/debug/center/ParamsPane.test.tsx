import type { ReactNode } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { EditorView } from '@codemirror/view';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugCallRequest } from '../../../core/ipc/generated/debug/DebugCallRequest';
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
};

vi.mock('../../../core/services/onebot-debug.service', () => ({
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
vi.mock('../../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: () => () => {},
}));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { debugWorkspaceStore, defaultWorkspace } from '../../../hooks/debug/debugWorkspaceStore';
import { _resetDebugCatalogForTests } from '../../../hooks/debug/useDebugCatalog';
import { CenterColumn } from './CenterColumn';
import { _resetSeedStateForTests } from './seedState';

const BOT: DebugTarget = {
    bot_id: 'bot-sl',
    name: '小雪',
    qq_id: 2854196310,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};

const SEND_GROUP_MSG: DebugActionSpec = {
    name: 'send_group_msg',
    aliases: [],
    summary: '发群消息',
    description: null,
    category: 'message',
    safety: 'side_effect',
    stream: false,
    supported: true,
    params_schema: {
        type: 'object',
        properties: {
            group_id: { type: 'integer', 'x-ncd-role': 'group_id', description: '群号' },
            message: {
                'x-ncd-role': 'message',
                anyOf: [{ type: 'array', items: { type: 'object' } }, { type: 'string' }],
            },
            auto_escape: { type: 'boolean', default: false },
        },
        required: ['group_id', 'message'],
    },
    returns_schema: { type: 'object', properties: { message_id: { type: 'integer' } } },
    returns_text: null,
    return_example: null,
    examples: [],
    error_examples: [],
    invariants: [],
    other_backend: null,
    source: 'live',
};

function workspace(params_text: string): DebugWorkspace {
    return {
        ...defaultWorkspace(),
        tabs: [
            { id: 't1', action: 'send_group_msg', params_text, timeout_ms: null, channel: null },
        ],
        active_tab: 't1',
        selected_bot: BOT.bot_id,
    };
}

function tabText(): string {
    return debugWorkspaceStore.getSnapshot().ws.tabs.find((t) => t.id === 't1')?.params_text ?? '';
}

function editorView(container: HTMLElement): EditorView {
    const el = container.querySelector('.cm-editor[aria-label], .cm-editor') as HTMLElement | null;
    const view = el ? EditorView.findFromDOM(el) : null;
    if (!view) throw new Error('找不到 JSON 编辑器');
    return view;
}

function typeIntoEditor(view: EditorView, text: string) {
    act(() => {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
    });
}

async function renderColumn(params_text = '{}') {
    service.workspace.mockResolvedValue(workspace(params_text));
    await debugWorkspaceStore.load();
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(
        <CenterColumn
            target={BOT}
            callChannel={{ kind: 'auto' }}
            onOpenPalette={vi.fn()}
            onRevealCallChannel={vi.fn()}
        />,
        { wrapper },
    );
}

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
    // CodeMirror 量文字尺寸时用到，jsdom 没有
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
    debugWorkspaceStore._reset();
    _resetDebugCatalogForTests();
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
    service.catalog.mockResolvedValue({
        backend: 'snowluma',
        source: 'live',
        snapshot_version: 't',
        actions: [],
    });
    service.describe.mockImplementation(async (_bot: string, _backend: string, name: string) =>
        name === 'send_group_msg' ? SEND_GROUP_MSG : null,
    );
    service.collections.mockResolvedValue({ version: 1, folders: [], requests: [] });
    // 群选择器会去拉 get_group_list
    service.call.mockImplementation(async (req: DebugCallRequest) => ({
        request_id: req.request_id,
        result: {
            kind: 'ok',
            outcome: {
                ok: true,
                status: 'ok',
                retcode: 0,
                data: [{ group_id: 100001, group_name: '测试群 1', member_count: 3 }],
                message: '',
                wording: '',
                raw: {},
                elapsed_ms: 5,
                channel: { kind: 'internal' },
                size_bytes: 10,
                truncated: false,
            },
        },
    }));
});

afterEach(() => {
    cleanup();
});

describe('参数表单 ⇄ JSON 同步', () => {
    it('说明读到后按必填项填上初始参数（算作没改过）；整数群号空着不填 0，表单上标「必填」', async () => {
        await renderColumn('{}');
        await waitFor(() => expect(JSON.parse(tabText())).toEqual({ message: '' }));
        await waitFor(() =>
            expect(document.getElementById('debug-param-t1-group_id')).toHaveAccessibleDescription(
                /必填/,
            ),
        );
        // 表单里出现各个字段
        expect(document.getElementById('debug-param-t1-group_id')).toBeInTheDocument();
        expect(document.getElementById('debug-param-t1-auto_escape')).toBeInTheDocument();
        // 初始文本不算改过：标签上没有「改过」的点
        expect(screen.queryByLabelText('参数改过')).not.toBeInTheDocument();
    });

    it('在表单里填群号，JSON 文本跟着改', async () => {
        const user = userEvent.setup();
        const { container } = await renderColumn('{\n  "group_id": 0,\n  "message": "hi"\n}');
        const input = await waitFor(() => {
            const el = document.getElementById(
                'debug-param-t1-group_id',
            ) as HTMLInputElement | null;
            if (!el) throw new Error('还没画出表单');
            return el;
        });
        await user.clear(input);
        await user.type(input, '100001');

        expect(JSON.parse(tabText())).toEqual({ group_id: 100001, message: 'hi' });
        // JSON 编辑器里的文本也是同一份
        expect(editorView(container).state.doc.toString()).toBe(tabText());
        // 群名作为提示显示在输入框里（候选来自 get_group_list）
        await waitFor(() => expect(screen.getAllByText('测试群 1').length).toBeGreaterThan(0));
        expect(screen.getByLabelText('参数改过')).toBeInTheDocument();
    });

    it('在 JSON 里改，表单跟着变；布尔开关写回 JSON', async () => {
        const user = userEvent.setup();
        const { container } = await renderColumn('{\n  "group_id": 1,\n  "message": "x"\n}');
        await waitFor(() =>
            expect(document.getElementById('debug-param-t1-group_id')).toBeInTheDocument(),
        );

        typeIntoEditor(editorView(container), '{"group_id": 123456, "message": "早上好"}');
        expect(tabText()).toBe('{"group_id": 123456, "message": "早上好"}');
        await waitFor(() =>
            expect(
                (document.getElementById('debug-param-t1-group_id') as HTMLInputElement).value,
            ).toBe('123456'),
        );
        expect(
            (document.getElementById('debug-param-t1-message') as HTMLTextAreaElement).value,
        ).toBe('早上好');

        await user.click(document.getElementById('debug-param-t1-auto_escape') as HTMLElement);
        expect(JSON.parse(tabText())).toEqual({
            group_id: 123456,
            message: '早上好',
            auto_escape: true,
        });
    });

    it('JSON 写坏时表单锁住并提示出错的行，改好后恢复；文本不丢', async () => {
        const user = userEvent.setup();
        const { container } = await renderColumn('{\n  "group_id": 1,\n  "message": "x"\n}');
        await waitFor(() =>
            expect(document.getElementById('debug-param-t1-group_id')).toBeInTheDocument(),
        );

        const broken = '{\n  "group_id": 1,\n  "message": \n}';
        typeIntoEditor(editorView(container), broken);
        expect(await screen.findByText('JSON 第 4 行有错，改好后表单恢复')).toBeInTheDocument();
        expect(tabText()).toBe(broken);

        await user.click(screen.getByRole('button', { name: /去 JSON 修改/ }));
        expect(screen.getByRole('radio', { name: 'JSON' })).toHaveAttribute('aria-checked', 'true');

        typeIntoEditor(editorView(container), '{"group_id": 1, "message": "ok"}');
        await waitFor(() => expect(screen.queryByText(/改好后表单恢复/)).not.toBeInTheDocument());
        await user.click(screen.getByRole('radio', { name: '表单' }));
        expect(
            (document.getElementById('debug-param-t1-message') as HTMLTextAreaElement).value,
        ).toBe('ok');
    });

    it('说明里没有的键列在「其它参数」里，点了去 JSON 编辑', async () => {
        const user = userEvent.setup();
        await renderColumn('{\n  "group_id": 1,\n  "message": "x",\n  "echo_me": 7\n}');
        const section = await screen.findByRole('region', { name: '其它参数' });
        expect(section).toHaveTextContent('echo_me');
        await user.click(screen.getByRole('button', { name: /在 JSON 里编辑/ }));
        expect(screen.getByRole('radio', { name: 'JSON' })).toHaveAttribute('aria-checked', 'true');
    });

    it('必填项空着时字段下标红，发送按钮变成「仍然发送」', async () => {
        const { container } = await renderColumn('{\n  "group_id": 1,\n  "message": ""\n}');
        expect(await screen.findByRole('button', { name: /仍然发送/ })).toBeEnabled();
        expect(screen.getAllByText('必填').length).toBeGreaterThan(0);
        // 问题不是 live 的 alert（敲字时一会儿有一会儿没，不该反复打断朗读），而是挂在字段的描述上，聚焦字段时读到
        expect(container.querySelector('[data-param] [role="alert"]')).toBeNull();
        const message = document.getElementById('debug-param-t1-message') as HTMLTextAreaElement;
        expect(message).toHaveAccessibleDescription(/必填/);
        const describedBy = message.getAttribute('aria-describedby');
        expect(describedBy).toBeTruthy();
        expect(document.getElementById(describedBy!)).toHaveTextContent('必填');
    });
});

const SET_GROUP_KICK: DebugActionSpec = {
    ...SEND_GROUP_MSG,
    name: 'set_group_kick',
    summary: '踢人',
    safety: 'side_effect',
    params_schema: {
        type: 'object',
        properties: {
            group_id: { type: 'integer', 'x-ncd-role': 'group_id' },
            user_id: { type: 'integer', 'x-ncd-role': 'member_id' },
        },
        required: ['group_id', 'user_id'],
    },
    returns_schema: null,
};

function memberListCalls(): DebugCallRequest[] {
    return service.call.mock.calls
        .map((c) => c[0] as DebugCallRequest)
        .filter((r) => r.action === 'get_group_member_list');
}

describe('说明读取中与一次性的跳转', () => {
    it('说明还在读时停在「表单」并显示读取中；读到后不自动切视图', async () => {
        let resolveSpec: (s: DebugActionSpec) => void = () => {};
        service.describe.mockImplementation(
            () =>
                new Promise<DebugActionSpec>((resolve) => {
                    resolveSpec = resolve;
                }),
        );
        const user = userEvent.setup();
        await renderColumn('{}');
        // 读取中「表单」照样能选（上一个用例可能把偏好留在了 JSON）
        const formRadio = await screen.findByRole('radio', { name: '表单' });
        expect(formRadio).toBeEnabled();
        await user.click(formRadio);
        expect(formRadio).toHaveAttribute('aria-checked', 'true');
        expect(screen.getByText(/正在读取接口说明，读到就出表单/)).toBeVisible();
        // 文档子页也不因为「还没有说明」被禁用
        expect(screen.getByRole('radio', { name: /文档/ })).toBeEnabled();

        await act(async () => {
            resolveSpec(SEND_GROUP_MSG);
        });
        await waitFor(() =>
            expect(document.getElementById('debug-param-t1-group_id')).toBeInTheDocument(),
        );
        expect(screen.getByRole('radio', { name: '表单' })).toHaveAttribute('aria-checked', 'true');
    });

    it('说明还在读时不拿 `{}` 比：带参数恢复的标签不会被误标成改过', async () => {
        service.describe.mockImplementation(() => new Promise<DebugActionSpec>(() => {}));
        await renderColumn('{\n  "group_id": 0,\n  "message": ""\n}');
        expect(await screen.findByText(/正在读取接口说明，读到就出表单/)).toBeInTheDocument();
        expect(screen.queryByLabelText('参数改过')).not.toBeInTheDocument();
    });

    it('点问题汇总聚焦对应字段只发生一次，之后来回切视图不再抢焦点', async () => {
        const user = userEvent.setup();
        await renderColumn('{\n  "group_id": 1,\n  "message": ""\n}');
        const jump = await screen.findByRole('button', { name: /^message/ });
        await user.click(jump);
        const message = document.getElementById('debug-param-t1-message') as HTMLTextAreaElement;
        await waitFor(() => expect(document.activeElement).toBe(message));

        await user.click(screen.getByRole('radio', { name: 'JSON' }));
        await user.click(screen.getByRole('radio', { name: '表单' }));
        // 等一帧：要是还会重放，焦点会在这一帧里被抢回到 message 上
        await act(async () => {
            await new Promise((r) => setTimeout(r, 30));
        });
        expect(document.activeElement).toBe(screen.getByRole('radio', { name: '表单' }));
    });

    it('「去 JSON 修改」只跳一次行，切回表单再切 JSON 不会把焦点抢进编辑器', async () => {
        const user = userEvent.setup();
        const { container } = await renderColumn('{\n  "group_id": 1,\n  "message": "x"\n}');
        await waitFor(() =>
            expect(document.getElementById('debug-param-t1-group_id')).toBeInTheDocument(),
        );
        typeIntoEditor(editorView(container), '{\n  "group_id": 1,\n');
        await user.click(await screen.findByRole('button', { name: /去 JSON 修改/ }));
        await waitFor(() => expect(container.querySelector('.cm-content')).toHaveFocus());

        await user.click(screen.getByRole('radio', { name: '表单' }));
        await user.click(screen.getByRole('radio', { name: 'JSON' }));
        await act(async () => {
            await new Promise((r) => setTimeout(r, 30));
        });
        expect(document.activeElement).toBe(screen.getByRole('radio', { name: 'JSON' }));
    });
});

describe('群成员选择器跟着群号走，但不跟每个按键', () => {
    it('群号停手之后才拉成员列表，敲的过程中不拉；没填完的群号不拉', async () => {
        const user = userEvent.setup();
        service.describe.mockImplementation(async (_b: string, _k: string, name: string) =>
            name === 'set_group_kick' ? SET_GROUP_KICK : null,
        );
        service.workspace.mockResolvedValue({
            ...workspace('{}'),
            tabs: [
                {
                    id: 't1',
                    action: 'set_group_kick',
                    params_text: '{}',
                    timeout_ms: null,
                    channel: null,
                },
            ],
        });
        await debugWorkspaceStore.load();
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        render(
            <QueryClientProvider client={client}>
                <TooltipProvider>
                    <CenterColumn
                        target={BOT}
                        callChannel={{ kind: 'auto' }}
                        onOpenPalette={vi.fn()}
                        onRevealCallChannel={vi.fn()}
                    />
                </TooltipProvider>
            </QueryClientProvider>,
        );
        const group = await waitFor(() => {
            const el = document.getElementById(
                'debug-param-t1-group_id',
            ) as HTMLInputElement | null;
            if (!el) throw new Error('还没画出表单');
            return el;
        });

        // 没填完：12
        await user.clear(group);
        await user.type(group, '12');
        await act(async () => {
            await new Promise((r) => setTimeout(r, 550));
        });
        expect(memberListCalls()).toHaveLength(0);
        expect(screen.getByText(/群号「12」看着还没填完/)).toBeInTheDocument();

        // 填完整：敲的过程中一次都不拉，停手 400ms 后拉一次
        await user.clear(group);
        await user.type(group, '100001');
        expect(memberListCalls()).toHaveLength(0);
        await waitFor(() => expect(memberListCalls()).toHaveLength(1), { timeout: 2000 });
        expect(memberListCalls()[0]!.params).toEqual({ group_id: 100001 });

        // 0 不拉
        await user.clear(group);
        await user.type(group, '0');
        await act(async () => {
            await new Promise((r) => setTimeout(r, 550));
        });
        expect(memberListCalls()).toHaveLength(1);
        expect(screen.getByText('先填 group_id，才能从群成员里挑')).toBeInTheDocument();
    });
});

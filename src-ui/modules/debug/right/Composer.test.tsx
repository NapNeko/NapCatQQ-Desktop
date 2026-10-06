import type { ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugEvent } from '../../../core/ipc/generated/debug/DebugEvent';
import type { DebugEventBatch } from '../../../core/ipc/generated/debug/DebugEventBatch';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

const service = {
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
    call: vi.fn(),
    describe: vi.fn(),
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
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { debugEventStore } from '../../../hooks/debug/debugEventStore';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { _resetDebugScrollMemoryForTests } from '../../../hooks/debug/debugScrollMemory';
import { RightColumn } from './RightColumn';
import { _resetComposerDraftsForTests } from './composerDrafts';

const BOT: DebugTarget = {
    bot_id: 'bot-1',
    name: '小雪',
    qq_id: 2854196310,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};

// 虚拟列表要尺寸才画行：聊天滚动区 600px，其余元素 60px
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).dataset.testid === 'chat-scroller' ? 600 : 60;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => 380,
    });
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
});

let push: ((batch: DebugEventBatch) => void) | null = null;

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    preferencesStore.setMotionEnabled(false);
    service.unsubscribe.mockResolvedValue(undefined);
    service.subscribe.mockImplementation(
        async (botId: string, _source: unknown, onBatch: (b: DebugEventBatch) => void) => {
            push = onBatch;
            return {
                subscription_id: 'sub-1',
                receiver: {
                    bot_id: botId,
                    source: { kind: 'internal' },
                    state: { state: 'connected' },
                    buffered: 0,
                    dropped_total: 0,
                    first_seq: 1,
                    viewers: 1,
                },
            };
        },
    );
    service.call.mockImplementation(async (req: { request_id: string; action: string }) => {
        if (req.action === 'get_group_member_list') {
            return ok(req.request_id, [
                { user_id: 10003, nickname: '阿强', card: '', role: 'member' },
                { user_id: 10004, nickname: 'Bob', card: '后端搬砖', role: 'admin' },
            ]);
        }
        return ok(req.request_id, { message_id: 9001 });
    });
});

afterEach(() => {
    cleanup();
    debugEventStore._reset();
    debugWorkspaceStore._reset();
    _resetDebugScrollMemoryForTests();
    _resetComposerDraftsForTests();
    preferencesStore.reset();
    push = null;
});

function ok(requestId: string, data: unknown) {
    return {
        request_id: requestId,
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
                size_bytes: 10,
                truncated: false,
            },
        },
    };
}

const T0 = Date.now() - 60_000;

function ob11(seq: number, payload: Record<string, unknown>): DebugEvent {
    return { seq, at_ms: T0 + seq * 1000, body: { kind: 'ob11', payload } };
}

const GROUP_MSG = ob11(1, {
    post_type: 'message',
    message_type: 'group',
    self_id: BOT.qq_id,
    group_id: 100001,
    group_name: '测试群 1',
    user_id: 10001,
    message_id: 555,
    sender: { user_id: 10001, nickname: '小明', card: '', role: 'member' },
    message: [{ type: 'text', data: { text: '这个接口怎么用？' } }],
});

const PRIVATE_MSG = ob11(2, {
    post_type: 'message',
    message_type: 'private',
    self_id: BOT.qq_id,
    user_id: 10002,
    message_id: 556,
    sender: { user_id: 10002, nickname: 'Alice' },
    message: [{ type: 'text', data: { text: '在吗' } }],
});

async function seed(events: DebugEvent[]) {
    await debugEventStore.ensureReceiving(BOT.bot_id, { kind: 'auto' }, BOT.qq_id);
    act(() => push?.({ v: 1, bot_id: BOT.bot_id, events }));
    await waitFor(() =>
        expect(debugEventStore.getSnapshot().bots[BOT.bot_id]?.chat.items.length).toBe(
            events.length,
        ),
    );
}

function renderColumn() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(<RightColumn target={BOT} callChannel={{ kind: 'internal' }} />, { wrapper });
}

/** 聊天滚动区的几何：内容高度跟着虚拟列表撑出来的总高走，记下程序滚动的目标和当时的最底位置 */
function scrollMetrics(el: HTMLElement, view: number) {
    let top = 0;
    const calls: Array<{ to: number; max: number }> = [];
    const height = () =>
        parseFloat((el.firstElementChild as HTMLElement | null)?.style.height ?? '') || view;
    const maxTop = () => Math.max(0, height() - view);
    Object.defineProperty(el, 'scrollHeight', { configurable: true, get: height });
    Object.defineProperty(el, 'clientHeight', { configurable: true, get: () => view });
    Object.defineProperty(el, 'scrollTop', {
        configurable: true,
        get: () => top,
        set: (v: number) => {
            top = Math.max(0, Math.min(v, maxTop()));
        },
    });
    (el as HTMLElement & { scrollTo: (o: ScrollToOptions) => void }).scrollTo = (
        o: ScrollToOptions,
    ) => {
        if (typeof o.top !== 'number') return;
        calls.push({ to: o.top, max: maxTop() });
        top = Math.max(0, Math.min(o.top, maxTop()));
        fireEvent.scroll(el);
    };
    return {
        calls,
        get top() {
            return top;
        },
        pulledToBottom: (since: number) =>
            calls.slice(since).some((c) => c.to >= c.max - 1 && c.max > 0),
        scrollTo(value: number) {
            top = Math.max(0, Math.min(value, maxTop()));
            fireEvent.scroll(el);
        },
    };
}

const textbox = () => screen.getByRole('textbox', { name: /消息/ });

function lastSend() {
    const calls = service.call.mock.calls.map((c) => c[0] as { action: string });
    return [...calls].reverse().find((c) => c.action.startsWith('send_'));
}

describe('Composer（在右栏里）', () => {
    it('「全部」视图里没选气泡时置灰；点了气泡就发到它的会话并回复它：reply + at + text', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG, PRIVATE_MSG]);
        renderColumn();

        expect(textbox()).toBeDisabled();
        expect(textbox()).toHaveAttribute('placeholder', '先选一个会话，或点一条消息');

        await user.click(screen.getByText('这个接口怎么用？'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        expect(screen.getByText('发到 测试群 1')).toBeInTheDocument();
        expect(screen.getByText(/回复 小明：这个接口怎么用？/)).toBeInTheDocument();

        // 输入 @ 弹出群成员，选阿强
        await user.type(textbox(), '@');
        const list = await screen.findByRole('listbox', { name: '群成员' });
        await within(list).findByText('阿强');
        expect(service.call).toHaveBeenCalledWith(
            expect.objectContaining({
                action: 'get_group_member_list',
                params: { group_id: 100001 },
                origin: 'picker',
            }),
        );
        await user.click(within(list).getByText('阿强'));
        expect(textbox()).toHaveValue('@阿强 ');

        await user.type(textbox(), '你好');
        await user.keyboard('{Enter}');

        await waitFor(() => expect(lastSend()).toBeDefined());
        expect(lastSend()).toEqual(
            expect.objectContaining({
                bot_id: 'bot-1',
                channel: { kind: 'internal' },
                action: 'send_group_msg',
                origin: 'composer',
                timeout_ms: 30000,
                params: {
                    group_id: 100001,
                    message: [
                        { type: 'reply', data: { id: '555' } },
                        { type: 'at', data: { qq: '10003' } },
                        { type: 'text', data: { text: ' 你好' } },
                    ],
                },
            }),
        );
        // 发出去了：清空、不再回复
        await waitFor(() => expect(textbox()).toHaveValue(''));
        expect(screen.queryByText(/回复 小明/)).not.toBeInTheDocument();
    });

    it('「全部」视图里选的是私聊气泡：走 send_private_msg，发给对方', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG, PRIVATE_MSG]);
        renderColumn();

        await user.click(screen.getByText('在吗'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        expect(screen.getByText('发到 Alice')).toBeInTheDocument();
        // 私聊没有 @ 按钮
        expect(screen.queryByRole('button', { name: '@ 群成员' })).not.toBeInTheDocument();

        await user.type(textbox(), '在的');
        await user.keyboard('{Enter}');
        await waitFor(() => expect(lastSend()).toBeDefined());
        expect(lastSend()).toEqual(
            expect.objectContaining({
                action: 'send_private_msg',
                params: {
                    user_id: 10002,
                    message: [
                        { type: 'reply', data: { id: '556' } },
                        { type: 'text', data: { text: '在的' } },
                    ],
                },
            }),
        );
    });

    it('发送失败时字留着，下面写明原因', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(async (req: { request_id: string }) => ({
            request_id: req.request_id,
            result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } },
        }));
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));

        await waitFor(() => expect(textbox()).toBeEnabled());
        expect(textbox()).toHaveAttribute('placeholder', '发到「测试群 1」，Enter 发送');
        await user.type(textbox(), '测试一下');
        await user.keyboard('{Enter}');

        expect(await screen.findByRole('alert')).toHaveTextContent('等太久了，调用超时');
        expect(textbox()).toHaveValue('测试一下');
    });

    it('发出去被上游拒了（回包比事件先到）：输入框下写原因，时间线上那条气泡也写上游的说明', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(async (req: { request_id: string }) => {
            const base = ok(req.request_id, null);
            return {
                ...base,
                result: {
                    kind: 'ok',
                    outcome: {
                        ...base.result.outcome,
                        ok: false,
                        status: 'failed',
                        retcode: 1200,
                        wording: '消息内容为空',
                        message: 'EMPTY',
                    },
                },
            };
        });
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), '测试一下');
        await user.keyboard('{Enter}');
        expect(await screen.findByRole('alert')).toHaveTextContent(
            '没发出去：retcode 1200 · 消息内容为空',
        );

        const sent = lastSend() as unknown as { request_id: string; params: unknown };
        act(() =>
            push?.({
                v: 1,
                bot_id: BOT.bot_id,
                events: [
                    {
                        seq: 5,
                        at_ms: T0 + 5000,
                        body: {
                            kind: 'call',
                            record: {
                                request_id: sent.request_id,
                                origin: 'composer',
                                action: 'send_group_msg',
                                params: sent.params,
                                ok: false,
                                retcode: 1200,
                                elapsed_ms: 30,
                                message_id: null,
                                error: null,
                                channel: { kind: 'internal' },
                            },
                        },
                    },
                ],
            }),
        );
        expect(
            await screen.findByText('↗ send_group_msg · ✗ retcode 1200 · 消息内容为空'),
        ).toBeInTheDocument();
    });

    it('Shift + Enter 换行，不发送', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), '第一行{Shift>}{Enter}{/Shift}第二行');
        expect(textbox()).toHaveValue('第一行\n第二行');
        expect(lastSend()).toBeUndefined();
    });
    it('「全部」视图里取消回复（× / Esc）、发出去之后，只清回复，「发到」的会话还在；点掉「发到」才置灰', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG, PRIVATE_MSG]);
        renderColumn();

        await user.click(screen.getByText('这个接口怎么用？'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.click(screen.getByRole('button', { name: '不回复了' }));
        expect(screen.queryByText(/回复 小明/)).not.toBeInTheDocument();
        expect(screen.getByText('发到 测试群 1')).toBeInTheDocument();
        expect(textbox()).toBeEnabled();

        // 再点一次气泡又回复它；Esc 同样只清回复
        await user.click(screen.getByText('这个接口怎么用？'));
        expect(screen.getByText(/回复 小明/)).toBeInTheDocument();
        await user.type(textbox(), '{Escape}');
        expect(screen.queryByText(/回复 小明/)).not.toBeInTheDocument();
        expect(textbox()).toBeEnabled();

        await user.type(textbox(), '收到');
        await user.keyboard('{Enter}');
        await waitFor(() => expect(lastSend()).toBeDefined());
        expect(lastSend()).toEqual(
            expect.objectContaining({
                action: 'send_group_msg',
                params: { group_id: 100001, message: [{ type: 'text', data: { text: '收到' } }] },
            }),
        );
        await waitFor(() => expect(textbox()).toHaveValue(''));
        // 发完还能接着发
        expect(screen.getByText('发到 测试群 1')).toBeInTheDocument();
        expect(textbox()).toBeEnabled();

        await user.click(screen.getByRole('button', { name: '不发到这里' }));
        expect(textbox()).toBeDisabled();
        expect(screen.queryByText('发到 测试群 1')).not.toBeInTheDocument();
    });

    it('发出去之后回到最新：翻上去了也会滚回底部，看得见自己的气泡', async () => {
        const user = userEvent.setup();
        const many: DebugEvent[] = [];
        for (let i = 1; i <= 30; i += 1) {
            many.push(
                ob11(i, {
                    post_type: 'message',
                    message_type: 'group',
                    self_id: BOT.qq_id,
                    group_id: 100001,
                    group_name: '测试群 1',
                    user_id: 10001,
                    message_id: 600 + i,
                    sender: { user_id: 10001, nickname: '小明', card: '', role: 'member' },
                    message: [{ type: 'text', data: { text: `第 ${i} 句` } }],
                }),
            );
        }
        await seed(many);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());

        const scroller = screen.getByTestId('chat-scroller');
        const m = scrollMetrics(scroller, 600);
        fireEvent.wheel(scroller, { deltaY: -300 });
        m.scrollTo(0);
        expect(m.top).toBe(0);

        const since = m.calls.length;
        await user.type(textbox(), '我来了');
        await user.keyboard('{Enter}');
        await waitFor(() => expect(lastSend()).toBeDefined());
        await waitFor(() => expect(m.pulledToBottom(since)).toBe(true));
    });

    it('@ 弹层默认高亮第一个成员，不是「全体成员」；回车 @ 的是他', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());

        await user.type(textbox(), '@');
        const list = await screen.findByRole('listbox', { name: '群成员' });
        await within(list).findByText('阿强');
        expect(within(list).getByRole('option', { name: /全体成员/ })).toHaveAttribute(
            'aria-selected',
            'false',
        );
        expect(within(list).getByRole('option', { name: /阿强/ })).toHaveAttribute(
            'aria-selected',
            'true',
        );
        await user.keyboard('{Enter}');
        expect(textbox()).toHaveValue('@阿强 ');
    });

    it('@ 弹层弹在停着的指针底下：指针没动不换高亮，回车 @ 的还是第一个成员', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());

        await user.type(textbox(), '@');
        const list = await screen.findByRole('listbox', { name: '群成员' });
        await within(list).findByText('阿强');
        const all = within(list).getByRole('option', { name: /全体成员/ });
        // 指针本来就停在这里：mouseenter，以及浏览器补发的坐标没变的 mousemove
        fireEvent.mouseEnter(all);
        fireEvent.mouseOver(all);
        fireEvent.mouseMove(all, { clientX: 40, clientY: 20 });
        fireEvent.mouseMove(all, { clientX: 40, clientY: 20 });
        expect(all).toHaveAttribute('aria-selected', 'false');
        await user.keyboard('{Enter}');
        expect(textbox()).toHaveValue('@阿强 ');
    });

    it('指针真的移到某一项上才高亮它', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());

        await user.type(textbox(), '@');
        const list = await screen.findByRole('listbox', { name: '群成员' });
        await within(list).findByText('后端搬砖');
        const bob = within(list).getByRole('option', { name: /后端搬砖/ });
        fireEvent.mouseMove(bob, { clientX: 40, clientY: 60 });
        fireEvent.mouseMove(bob, { clientX: 41, clientY: 62 });
        expect(bob).toHaveAttribute('aria-selected', 'true');
        await user.keyboard('{Enter}');
        expect(textbox()).toHaveValue('@后端搬砖 ');
    });

    it('发送按钮用统一的悬停提示，不用原生 title', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), '嗨');

        const button = screen.getByRole('button', { name: '发送' });
        expect(button).not.toHaveAttribute('title');
        await user.hover(button);
        // jsdom 里 GSAP 的进场样式让气泡算作不可见，按 hidden 也一起找
        expect(await screen.findByRole('tooltip', { hidden: true })).toHaveTextContent(
            '发送（Enter）',
        );
        expect(button).toHaveAttribute('aria-describedby');
    });

    it('群里重名的人：@ 进来时带上 QQ 号，发出去对得上人', async () => {
        const user = userEvent.setup();
        service.call.mockImplementation(async (req: { request_id: string; action: string }) =>
            req.action === 'get_group_member_list'
                ? ok(req.request_id, [
                      { user_id: 10006, nickname: '张三', card: '', role: 'member' },
                      { user_id: 10007, nickname: '张三', card: '', role: 'member' },
                  ])
                : ok(req.request_id, { message_id: 9002 }),
        );
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());

        await user.type(textbox(), '@');
        const list = await screen.findByRole('listbox', { name: '群成员' });
        await waitFor(() => expect(within(list).getAllByText('张三')).toHaveLength(2));
        await user.click(within(list).getAllByText('张三')[1] as HTMLElement);
        expect(textbox()).toHaveValue('@张三(10007) ');
        await user.type(textbox(), '你好');
        await user.keyboard('{Enter}');
        await waitFor(() => expect(lastSend()).toBeDefined());
        expect(lastSend()).toEqual(
            expect.objectContaining({
                params: {
                    group_id: 100001,
                    message: [
                        { type: 'at', data: { qq: '10007' } },
                        { type: 'text', data: { text: ' 你好' } },
                    ],
                },
            }),
        );
    });

    it('输入法选字时的回车（keyCode 229）不发送', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), 'ni');
        fireEvent.keyDown(textbox(), { key: 'Enter', keyCode: 229 });
        expect(lastSend()).toBeUndefined();
        expect(textbox()).toHaveValue('ni');
    });

    it('构建器拼装图片：手打文字并进段列表，发出去是 reply + text + image；发完构建内容清空', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        await user.click(screen.getByText('这个接口怎么用？'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), '看看这个');

        // 打开构建器，加一个图片段，写回
        await user.click(screen.getByRole('button', { name: '消息构建器' }));
        const dialog = await screen.findByRole('dialog', { name: '消息构建器' });
        expect(within(dialog).getByLabelText('文字内容')).toHaveValue('看看这个');
        await user.click(within(dialog).getByRole('combobox'));
        await user.click(within(await screen.findByRole('listbox')).getByText('图片'));
        fireEvent.change(within(dialog).getByLabelText('图片地址'), {
            target: { value: 'http://a/1.png' },
        });
        await user.click(within(dialog).getByRole('button', { name: '使用这些段' }));
        await waitFor(() =>
            expect(screen.queryByRole('dialog', { name: '消息构建器' })).not.toBeInTheDocument(),
        );

        // 文字进了构建内容摘要，输入框清空；发出去顺序 = 回复 + 文字 + 图片
        expect(screen.getByRole('button', { name: '编辑构建内容' })).toHaveTextContent(
            '构建：看看这个[图片]',
        );
        expect(textbox()).toHaveValue('');
        await user.click(textbox());
        await user.keyboard('{Enter}');
        await waitFor(() => expect(lastSend()).toBeDefined());
        expect(lastSend()).toEqual(
            expect.objectContaining({
                action: 'send_group_msg',
                params: {
                    group_id: 100001,
                    message: [
                        { type: 'reply', data: { id: '555' } },
                        { type: 'text', data: { text: '看看这个' } },
                        { type: 'image', data: { file: 'http://a/1.png' } },
                    ],
                },
            }),
        );

        // 发完构建内容清空，下一条只有手打的字
        await waitFor(() =>
            expect(screen.queryByRole('button', { name: '编辑构建内容' })).not.toBeInTheDocument(),
        );
        await user.type(textbox(), '第二句');
        await user.keyboard('{Enter}');
        await waitFor(() => expect(textbox()).toHaveValue(''));
        expect(lastSend()).toEqual(
            expect.objectContaining({
                params: { group_id: 100001, message: [{ type: 'text', data: { text: '第二句' } }] },
            }),
        );
    });

    it('构建器里只有文字和 @：写回折叠进输入框，发出去等价于手打', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), '你好');

        await user.click(screen.getByRole('button', { name: '消息构建器' }));
        const dialog = await screen.findByRole('dialog', { name: '消息构建器' });
        await user.click(within(dialog).getByRole('combobox'));
        await user.click(within(await screen.findByRole('listbox')).getByText('@ 成员'));
        // @ 段挪到文本前面
        await user.click(within(dialog).getAllByRole('button', { name: '上移' })[1] as HTMLElement);
        fireEvent.change(within(dialog).getByLabelText('QQ 号'), { target: { value: '10003' } });
        await user.click(within(dialog).getByRole('button', { name: '使用这些段' }));
        await waitFor(() =>
            expect(screen.queryByRole('dialog', { name: '消息构建器' })).not.toBeInTheDocument(),
        );

        expect(textbox()).toHaveValue('@10003 你好');
        expect(screen.queryByRole('button', { name: '编辑构建内容' })).not.toBeInTheDocument();
        await user.keyboard('{Enter}');
        await waitFor(() => expect(lastSend()).toBeDefined());
        expect(lastSend()).toEqual(
            expect.objectContaining({
                params: {
                    group_id: 100001,
                    message: [
                        { type: 'at', data: { qq: '10003' } },
                        { type: 'text', data: { text: ' 你好' } },
                    ],
                },
            }),
        );
    });

    it('转回文字输入：有非文本段时先进构建器，删掉图片段后文字回到输入框', async () => {
        const user = userEvent.setup();
        await seed([GROUP_MSG]);
        renderColumn();
        act(() => debugEventStore.setActiveSession(BOT.bot_id, 'group:100001'));
        await waitFor(() => expect(textbox()).toBeEnabled());
        await user.type(textbox(), '看看这个');

        await user.click(screen.getByRole('button', { name: '消息构建器' }));
        let dialog = await screen.findByRole('dialog', { name: '消息构建器' });
        await user.click(within(dialog).getByRole('combobox'));
        await user.click(within(await screen.findByRole('listbox')).getByText('图片'));
        fireEvent.change(within(dialog).getByLabelText('图片地址'), {
            target: { value: 'http://a/1.png' },
        });
        await user.click(within(dialog).getByRole('button', { name: '使用这些段' }));
        await waitFor(() =>
            expect(screen.queryByRole('dialog', { name: '消息构建器' })).not.toBeInTheDocument(),
        );

        // 有图片段，点「转回文字输入」直接打开构建器
        await user.click(screen.getByRole('button', { name: '转回文字输入' }));
        dialog = await screen.findByRole('dialog', { name: '消息构建器' });
        // 删掉图片那一行，剩下的都是文字 → 写回折叠进输入框
        await user.click(
            within(dialog).getAllByRole('button', { name: '删掉这段' })[1] as HTMLElement,
        );
        await user.click(within(dialog).getByRole('button', { name: '使用这些段' }));
        await waitFor(() =>
            expect(screen.queryByRole('dialog', { name: '消息构建器' })).not.toBeInTheDocument(),
        );
        expect(textbox()).toHaveValue('看看这个');
        expect(screen.queryByRole('button', { name: '编辑构建内容' })).not.toBeInTheDocument();
    });

    it('接收状态胶囊写完整状态：重连中（第 n 次，x 秒后）/ 已停止：原因', async () => {
        await seed([GROUP_MSG]);
        renderColumn();
        act(() =>
            push?.({
                v: 1,
                bot_id: BOT.bot_id,
                events: [
                    {
                        seq: 10,
                        at_ms: T0 + 10_000,
                        body: {
                            kind: 'receiver',
                            state: { state: 'reconnecting', attempt: 2, retry_in_ms: 3000 },
                            source: { kind: 'internal' },
                        },
                    },
                ],
            }),
        );
        const chip = await screen.findByRole('button', {
            name: '接收状态：重连中（第 2 次，3 秒后），点击管理',
        });
        expect(chip).toHaveTextContent('重连中（第 2 次，3 秒后）');

        act(() =>
            push?.({
                v: 1,
                bot_id: BOT.bot_id,
                events: [
                    {
                        seq: 11,
                        at_ms: T0 + 11_000,
                        body: {
                            kind: 'receiver',
                            state: { state: 'stopped', reason: 'Bot 已停止' },
                            source: { kind: 'internal' },
                        },
                    },
                ],
            }),
        );
        expect(
            await screen.findByRole('button', { name: '接收状态：已停止：Bot 已停止，点击管理' }),
        ).toHaveTextContent('已停止：Bot 已停止');
    });
});

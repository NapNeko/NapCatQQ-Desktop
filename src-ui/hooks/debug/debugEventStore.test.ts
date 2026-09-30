import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DebugEvent } from '../../core/ipc/generated/debug/DebugEvent';
import type { DebugEventBatch } from '../../core/ipc/generated/debug/DebugEventBatch';
import type { DebugReceiverInfo } from '../../core/ipc/generated/debug/DebugReceiverInfo';
import type { DebugReceiverState } from '../../core/ipc/generated/debug/DebugReceiverState';
import type { DebugSubscribeResponse } from '../../core/ipc/generated/debug/DebugSubscribeResponse';

const subscribeMock = vi.fn();
const unsubscribeMock = vi.fn();
const stopReceiverMock = vi.fn();
const pushErrorBar = vi.fn();

vi.mock('../../core/services/onebot-debug.service', () => ({
    onebotDebugService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
        unsubscribe: (...args: unknown[]) => unsubscribeMock(...args),
        stopReceiver: (...args: unknown[]) => stopReceiverMock(...args),
    },
}));

vi.mock('../ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBar(...args),
}));

// 模块级的域事件桥：测试里自己决定什么时候发 bot_state_changed
const domainHandlers: Array<(event: { kind: string; snapshot: { bot_id: string; state: string } }) => void> = [];

vi.mock('../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: (fn: (event: { kind: string; snapshot: { bot_id: string; state: string } }) => void) => {
        domainHandlers.push(fn);
        return () => {};
    },
}));

const emitBotState = (botId: string, state: string): void => {
    for (const h of domainHandlers) h({ kind: 'bot_state_changed', snapshot: { bot_id: botId, state } });
};

import { debugEventStore as store } from './debugEventStore';

const BOT = 'bot-1';
const AUTO = { kind: 'auto' } as const;
const SELF = 10000;

const groupMessage = (seq: number, over: Record<string, unknown> = {}): DebugEvent => ({
    seq,
    at_ms: 1_700_000_000_000 + seq,
    body: {
        kind: 'ob11',
        payload: {
            post_type: 'message',
            message_type: 'group',
            self_id: SELF,
            user_id: 20001,
            group_id: 100001,
            group_name: '测试群',
            message_id: 5000 + seq,
            sender: { user_id: 20001, nickname: '小明', card: '' },
            message: [{ type: 'text', data: { text: `第 ${seq} 条` } }],
            ...over,
        },
    },
});

const receiverEvent = (seq: number, state: DebugReceiverState): DebugEvent => ({
    seq,
    at_ms: 1_700_000_000_000 + seq,
    body: { kind: 'receiver', state, source: AUTO },
});

const batch = (events: DebugEvent[]): DebugEventBatch => ({ v: 1, bot_id: BOT, events });

/** 输入框发的一条群消息，OB11 回 retcode 1200（事件里的调用记录不带 wording） */
const failedSend = (seq: number, requestId = 'req-1'): DebugEvent => ({
    seq,
    at_ms: 1_700_000_000_000 + seq,
    body: {
        kind: 'call',
        record: {
            request_id: requestId,
            origin: 'composer',
            action: 'send_group_msg',
            params: { group_id: 100001, message: [{ type: 'text', data: { text: '你好' } }] },
            ok: false,
            retcode: 1200,
            elapsed_ms: 40,
            message_id: null,
            error: null,
            channel: { kind: 'internal' },
        },
    },
});

const wordingOf = (requestId: string) => {
    for (const item of bot()?.chat.items ?? []) {
        if ((item.kind === 'message' || item.kind === 'call') && item.call?.requestId === requestId) return item.call.wording;
    }
    return 'missing';
};

const info = (over: Partial<DebugReceiverInfo> = {}): DebugReceiverInfo => ({
    bot_id: BOT,
    source: AUTO,
    state: { state: 'connected' },
    buffered: 0,
    dropped_total: 0,
    first_seq: 1,
    viewers: 1,
    ...over,
});

// 手动出帧：测试里自己决定「这一帧什么时候来」
let frames: Array<FrameRequestCallback | null> = [];
const runFrame = () => {
    const due = frames;
    frames = [];
    due.forEach((cb) => cb?.(performance.now()));
};

interface Wire {
    push: (events: DebugEvent[]) => void;
    subscriptionId: string;
}

/** 装一个 subscribe：可预先给积压（在回包前同步推，和浏览器预览的 mock 一样），返回的 wire 用来推实时批次 */
function installSubscribe(opts: { backlog?: DebugEvent[]; receiver?: DebugReceiverInfo } = {}): Wire[] {
    const wires: Wire[] = [];
    subscribeMock.mockImplementation(
        async (_bot: string, _source: unknown, onBatch: (b: DebugEventBatch) => void): Promise<DebugSubscribeResponse> => {
            if (opts.backlog?.length) onBatch(batch(opts.backlog));
            const subscriptionId = `sub-${wires.length + 1}`;
            wires.push({ push: (events) => onBatch(batch(events)), subscriptionId });
            return { subscription_id: subscriptionId, receiver: opts.receiver ?? info() };
        },
    );
    return wires;
}

const bot = () => store.getSnapshot().bots[BOT];

beforeEach(() => {
    subscribeMock.mockReset();
    unsubscribeMock.mockReset();
    unsubscribeMock.mockResolvedValue(undefined);
    stopReceiverMock.mockReset();
    stopReceiverMock.mockResolvedValue(undefined);
    pushErrorBar.mockReset();
    frames = [];
    store._reset();
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        frames.push(cb);
        return frames.length;
    });
    vi.stubGlobal('cancelAnimationFrame', (handle: number) => {
        if (handle >= 1 && handle <= frames.length) frames[handle - 1] = null;
    });
});

afterEach(() => {
    store._reset();
    vi.unstubAllGlobals();
});

describe('一帧一批', () => {
    it('一帧里到的多批事件只 reduce + 通知一次，且只排一个帧回调', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        const listener = vi.fn();
        store.subscribe(listener);
        listener.mockClear();

        wires[0].push([groupMessage(1), groupMessage(2)]);
        wires[0].push([groupMessage(3)]);
        wires[0].push([groupMessage(4)]);

        expect(frames.filter(Boolean)).toHaveLength(1);
        expect(bot().chat.items).toHaveLength(0);
        expect(listener).not.toHaveBeenCalled();

        runFrame();

        expect(bot().chat.items).toHaveLength(4);
        expect(bot().chat.lastSeq).toBe(4);
        expect(listener).toHaveBeenCalledTimes(1);
    });

    it('空闲清扫的空批次是真的什么都不做：不排帧、不 reduce、不通知、快照不换', async () => {
        vi.useFakeTimers();
        try {
            const wires = installSubscribe();
            await store.ensureReceiving(BOT, AUTO, SELF);
            wires[0].push([groupMessage(1)]);
            runFrame();
            await vi.advanceTimersByTimeAsync(600);

            const listener = vi.fn();
            store.subscribe(listener);
            const snapshot = store.getSnapshot();
            const chat = bot().chat;

            for (let i = 0; i < 5; i += 1) wires[0].push([]);
            expect(frames.filter(Boolean)).toHaveLength(0);
            expect(vi.getTimerCount()).toBe(0);
            runFrame();
            await vi.advanceTimersByTimeAsync(600);

            expect(listener).not.toHaveBeenCalled();
            expect(store.getSnapshot()).toBe(snapshot);
            expect(bot().chat).toBe(chat);
        } finally {
            vi.useRealTimers();
        }
    });

    it('下一帧的批次再排新帧；窗口在后台不出帧时靠兜底定时器也会应用', async () => {
        vi.useFakeTimers();
        try {
            const wires = installSubscribe();
            await store.ensureReceiving(BOT, AUTO, SELF);

            wires[0].push([groupMessage(1)]);
            runFrame();
            wires[0].push([groupMessage(2)]);
            expect(bot().chat.items).toHaveLength(1);

            // 不再手动出帧，模拟后台不出帧
            await vi.advanceTimersByTimeAsync(600);
            expect(bot().chat.items).toHaveLength(2);
        } finally {
            vi.useRealTimers();
        }
    });
});

describe('订阅与积压', () => {
    it('ensureReceiving 幂等：并发和重复调用只订一次', async () => {
        installSubscribe();

        await Promise.all([
            store.ensureReceiving(BOT, AUTO, SELF),
            store.ensureReceiving(BOT, AUTO, SELF),
        ]);
        await store.ensureReceiving(BOT, AUTO, SELF);

        expect(subscribeMock).toHaveBeenCalledTimes(1);
        expect(bot().subscriptionId).toBe('sub-1');
        expect(bot().receiver).toEqual(info());
        expect(bot().chat.selfId).toBe(SELF);
    });

    it('回包前同步补来的积压也会应用', async () => {
        installSubscribe({ backlog: [groupMessage(1), groupMessage(2)], receiver: info({ buffered: 2 }) });

        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();

        expect(bot().chat.items).toHaveLength(2);
    });

    it('退订后重新订阅补到同样的积压：按 seq 去重，不会出重复气泡', async () => {
        const backlog = [groupMessage(1), groupMessage(2), groupMessage(3)];
        installSubscribe({ backlog, receiver: info({ buffered: 3 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();
        expect(bot().chat.items).toHaveLength(3);

        store.releaseView(BOT);
        expect(unsubscribeMock).toHaveBeenCalledWith('sub-1');
        expect(bot().subscriptionId).toBeNull();

        installSubscribe({ backlog: [...backlog, groupMessage(4)], receiver: info({ buffered: 4 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();

        expect(bot().chat.items).toHaveLength(4);
        expect(bot().chat.items.map((i) => i.seq)).toEqual([1, 2, 3, 4]);
    });

    it('释放后旧订阅晚到的批次被丢弃', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        store.releaseView(BOT);

        wires[0].push([groupMessage(1)]);
        runFrame();

        expect(bot().chat.items).toHaveLength(0);
    });

    it('释放时队列里已到的事件先应用掉', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([groupMessage(1)]);

        store.releaseView(BOT);

        expect(bot().chat.items).toHaveLength(1);
        expect(frames.filter(Boolean)).toHaveLength(0);
    });

    it('等订阅回包的时候页面已经走了：回包一到就退订，不留观众', async () => {
        let resolveSubscribe!: (r: DebugSubscribeResponse) => void;
        subscribeMock.mockReturnValue(new Promise<DebugSubscribeResponse>((r) => (resolveSubscribe = r)));
        const pending = store.ensureReceiving(BOT, AUTO, SELF);

        store.releaseView(BOT);
        resolveSubscribe({ subscription_id: 'late', receiver: info() });
        await pending;

        expect(unsubscribeMock).toHaveBeenCalledWith('late');
        expect(bot().subscriptionId).toBeNull();
    });

    it('前一个订阅还在路上、又来了另一条通道的请求：等的时候页面走了（releaseView）就不再订', async () => {
        let resolveFirst!: (r: DebugSubscribeResponse) => void;
        subscribeMock.mockReturnValueOnce(new Promise<DebugSubscribeResponse>((r) => (resolveFirst = r)));
        const first = store.ensureReceiving(BOT, AUTO, SELF);
        const queued = store.ensureReceiving(BOT, { kind: 'ws', name: 'main' }, SELF);

        store.releaseView(BOT);
        resolveFirst({ subscription_id: 'first', receiver: info() });
        await Promise.all([first, queued]);

        expect(subscribeMock).toHaveBeenCalledTimes(1);
        expect(unsubscribeMock).toHaveBeenCalledWith('first');
        expect(bot().subscriptionId).toBeNull();
    });

    it('排队等待期间页面没走：照常换到新通道；连着排了两个不同通道时最后一个说了算', async () => {
        let resolveFirst!: (r: DebugSubscribeResponse) => void;
        subscribeMock.mockReturnValueOnce(new Promise<DebugSubscribeResponse>((r) => (resolveFirst = r)));
        const first = store.ensureReceiving(BOT, AUTO, SELF);
        const second = store.ensureReceiving(BOT, { kind: 'ws', name: 'a' }, SELF);
        const third = store.ensureReceiving(BOT, { kind: 'http', name: 'b' }, SELF);
        installSubscribe();

        resolveFirst({ subscription_id: 'first', receiver: info() });
        await Promise.all([first, second, third]);

        expect(subscribeMock.mock.calls.map((c) => c[1])).toEqual([AUTO, { kind: 'ws', name: 'a' }, { kind: 'http', name: 'b' }]);
        expect(bot().subscriptionId).toBe('sub-2');
    });

    it('换事件通道：退掉旧订阅，按新通道重订', async () => {
        installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);

        await store.ensureReceiving(BOT, { kind: 'ws', name: 'main' }, SELF);

        expect(unsubscribeMock).toHaveBeenCalledWith('sub-1');
        expect(subscribeMock).toHaveBeenCalledTimes(2);
        expect(subscribeMock.mock.calls[1][1]).toEqual({ kind: 'ws', name: 'main' });
        expect(bot().subscriptionId).toBe('sub-2');
    });

    it('订阅失败：写进 error，弹错误条，不抛；之后可以重试', async () => {
        subscribeMock.mockRejectedValueOnce('Bot 未运行');

        await store.ensureReceiving(BOT, AUTO, SELF);

        expect(bot().error).toBe('Bot 未运行');
        expect(bot().subscriptionId).toBeNull();
        expect(pushErrorBar).toHaveBeenCalledWith(expect.objectContaining({ key: `debug-receive:${BOT}` }));

        installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        expect(bot().subscriptionId).toBe('sub-1');
        expect(bot().error).toBeUndefined();
    });
});

describe('接收器 Stopped 之后重新订阅', () => {
    it('收到 Stopped：清掉订阅和接收器状态；之后 ensureReceiving 会重新订', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);

        wires[0].push([receiverEvent(1, { state: 'stopped', reason: 'Bot 已停止' })]);
        runFrame();

        expect(bot().subscriptionId).toBeNull();
        expect(bot().receiver?.state).toEqual({ state: 'stopped', reason: 'Bot 已停止' });
        expect(unsubscribeMock).toHaveBeenCalledWith('sub-1');

        await store.ensureReceiving(BOT, AUTO, SELF);
        expect(subscribeMock).toHaveBeenCalledTimes(2);
        expect(bot().subscriptionId).toBe('sub-2');
    });

    it('重新订阅补来的积压里旧的 Stopped 不会把新订阅又清掉', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([receiverEvent(1, { state: 'stopped', reason: '空闲超时' })]);
        runFrame();

        // 后端接收器重启后：积压里还带着那条旧 Stopped（seq 1）和它之后的事件
        installSubscribe({
            backlog: [receiverEvent(1, { state: 'stopped', reason: '空闲超时' }), groupMessage(2)],
            receiver: info({ buffered: 2, first_seq: 1 }),
        });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();

        expect(bot().subscriptionId).toBe('sub-1');
        expect(bot().receiver?.state).toEqual({ state: 'connected' });
    });

    it('订阅之后的 Reconnecting / Connected 事件会更新接收器状态', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);

        wires[0].push([receiverEvent(1, { state: 'reconnecting', attempt: 2, retry_in_ms: 2000 })]);
        runFrame();
        expect(bot().receiver?.state).toEqual({ state: 'reconnecting', attempt: 2, retry_in_ms: 2000 });
        expect(bot().subscriptionId).toBe('sub-1');

        wires[0].push([receiverEvent(2, { state: 'connected' })]);
        runFrame();
        expect(bot().receiver?.state).toEqual({ state: 'connected' });
    });

    it('订阅回包还没到时推来的「已连接」不会被当成订阅前的旧事件丢掉', async () => {
        let send!: (events: DebugEvent[]) => void;
        let finish!: (res: DebugSubscribeResponse) => void;
        subscribeMock.mockImplementation(
            (_bot: unknown, _source: unknown, onBatch: (b: DebugEventBatch) => void) =>
                new Promise<DebugSubscribeResponse>((resolve) => {
                    send = (events) => onBatch(batch(events));
                    finish = resolve;
                }),
        );

        const pending = store.ensureReceiving(BOT, AUTO, SELF);
        // 回包还没回来，上游已经推来了一条「已连接」；订阅返回里记的却还是 connecting
        send([receiverEvent(1, { state: 'connected' })]);
        finish({ subscription_id: 'sub-1', receiver: info({ state: 'connecting' }) });
        await pending;
        runFrame();

        expect(bot().subscriptionId).toBe('sub-1');
        expect(bot().receiver?.state).toEqual({ state: 'connected' });
    });
});

describe('Bot 恢复运行后自动补订', () => {
    it('接收器被后端停掉时手里还挂着订阅：bot_state_changed 一报 running 就重新订', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([receiverEvent(1, { state: 'stopped', reason: 'Bot 已停止' })]);
        runFrame();
        expect(bot().subscriptionId).toBeNull();
        expect(bot().receiver?.state).toEqual({ state: 'stopped', reason: 'Bot 已停止' });
        expect(subscribeMock).toHaveBeenCalledTimes(1);

        emitBotState(BOT, 'running');
        await vi.waitUntil(() => store.getSnapshot().bots[BOT]?.subscriptionId === 'sub-2');

        expect(subscribeMock).toHaveBeenCalledTimes(2);
        expect(bot().receiver?.state).toEqual({ state: 'connected' });
    });

    it('本来就没订过（只是路过看着 Bot 列表）不补订', () => {
        emitBotState('bot-never-seen', 'running');
        expect(subscribeMock).not.toHaveBeenCalled();
    });

    it('手动停止之后不补订；「重新接收」解除手动标记后补订恢复', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        await store.stopReceiving(BOT);
        expect(bot().subscriptionId).toBeNull();

        emitBotState(BOT, 'running');
        expect(subscribeMock).toHaveBeenCalledTimes(1);

        await store.restartReceiving(BOT, AUTO, SELF);
        expect(bot().subscriptionId).toBe('sub-2');

        // 手动标记解除后：接收器又被后端停掉再报 running，补订照常
        wires[1].push([receiverEvent(2, { state: 'stopped', reason: 'Bot 已停止' })]);
        runFrame();
        expect(bot().subscriptionId).toBeNull();
        emitBotState(BOT, 'running');
        await vi.waitUntil(() => store.getSnapshot().bots[BOT]?.subscriptionId === 'sub-3');
        expect(subscribeMock).toHaveBeenCalledTimes(3);
    });
});

describe('已丢弃条数（时间线顶部的「更早的 N 条已丢弃」）', () => {
    it('接收器重建、缓冲接着编号：不算丢（旧条目还在时间线上）', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([groupMessage(1), groupMessage(2)]);
        runFrame();

        store.releaseView(BOT);
        // 接收器重建：seq 接着往后编，first_seq 恰好是下一条，什么都不能算丢
        installSubscribe({ backlog: [groupMessage(3)], receiver: info({ buffered: 1, first_seq: 3 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();

        expect(bot().unseenDropped).toBe(0);
        expect(bot().chat.items.map((i) => i.seq)).toEqual([1, 2, 3]);
    });

    it('离开期间被环形缓冲挤掉的才计数，多次订阅累计', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([groupMessage(1), groupMessage(2)]);
        runFrame();

        store.releaseView(BOT);
        // 离开期间又进了十几条，3–12 被挤掉，缓冲从 13 开始
        installSubscribe({ backlog: [groupMessage(13), groupMessage(14)], receiver: info({ buffered: 2, first_seq: 13 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();
        expect(bot().unseenDropped).toBe(10);

        store.releaseView(BOT);
        // 再离开一次，15–19 又被挤掉
        installSubscribe({ backlog: [groupMessage(20)], receiver: info({ buffered: 1, first_seq: 20 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();
        expect(bot().unseenDropped).toBe(15);
    });
});

describe('会话与停止', () => {
    it('setActiveSession 清未读；当前会话里来的消息不再计未读', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        // 正在看别的会话时，别人发来的消息才计未读（看「全部」时都在眼前，不计）
        store.setActiveSession(BOT, 'private:99');
        wires[0].push([groupMessage(1)]);
        runFrame();
        expect(bot().chat.sessions['group:100001'].unread).toBe(1);

        store.setActiveSession(BOT, 'group:100001');
        expect(bot().chat.sessions['group:100001'].unread).toBe(0);
        expect(bot().activeSession).toBe('group:100001');

        wires[0].push([groupMessage(2)]);
        runFrame();
        expect(bot().chat.sessions['group:100001'].unread).toBe(0);
    });

    it('stopReceiving：通知后端、退订、本地标成已停；手动停过之后自动路径不补订，「重新接收」才恢复；失败时弹错误条且保留订阅', async () => {
        installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);

        await store.stopReceiving(BOT);
        expect(stopReceiverMock).toHaveBeenCalledWith(BOT);
        expect(unsubscribeMock).toHaveBeenCalledWith('sub-1');
        expect(bot().subscriptionId).toBeNull();
        expect(bot().receiver?.state).toEqual({ state: 'stopped', reason: '已手动停止' });

        // 手动停止之后，自动路径（页面看到 running 翻转、Bot 恢复的补订）都不再偷偷订回来
        await store.ensureReceiving(BOT, AUTO, SELF);
        expect(subscribeMock).toHaveBeenCalledTimes(1);
        expect(bot().subscriptionId).toBeNull();
        emitBotState(BOT, 'running');
        await store.ensureReceiving(BOT, AUTO, SELF);
        expect(subscribeMock).toHaveBeenCalledTimes(1);

        // 顶栏「重新接收」是明确的用户意图，解除手动标记后才恢复
        await store.restartReceiving(BOT, AUTO, SELF);
        expect(bot().subscriptionId).toBe('sub-2');

        stopReceiverMock.mockRejectedValueOnce('后端没响应');
        await store.stopReceiving(BOT);
        expect(pushErrorBar).toHaveBeenCalledWith(expect.objectContaining({ key: `debug-stop-receiver:${BOT}` }));
        expect(bot().subscriptionId).toBe('sub-2');
    });

    it('clearChat 清掉条目但记下的人名留着，之后的通知照样写名字', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([groupMessage(1)]);
        runFrame();
        store.clearChat(BOT);
        expect(bot().chat.names.get(20001)).toBe('小明');

        wires[0].push([
            {
                seq: 2,
                at_ms: 1_700_000_000_002,
                body: { kind: 'ob11', payload: { post_type: 'notice', notice_type: 'notify', sub_type: 'poke', group_id: 100001, user_id: 20001, target_id: SELF } },
            },
        ]);
        runFrame();
        expect(bot().chat.items).toMatchObject([{ kind: 'notice', text: `小明 戳了戳 ${SELF}` }]);
    });

    it('clearChat 清屏但保留 lastSeq，重新补积压不会把旧内容补回来', async () => {
        const backlog = [groupMessage(1), groupMessage(2)];
        installSubscribe({ backlog, receiver: info({ buffered: 2 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();

        store.clearChat(BOT);
        expect(bot().chat.items).toHaveLength(0);
        expect(bot().chat.selfId).toBe(SELF);

        store.releaseView(BOT);
        installSubscribe({ backlog: [...backlog, groupMessage(3)], receiver: info({ buffered: 3 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();

        expect(bot().chat.items.map((i) => i.seq)).toEqual([3]);
    });
});

describe('失败调用的说明（wording）：回包和事件谁先到都能挂上', () => {
    it('事件先到、已经画在时间线上：回包来了直接补上', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([failedSend(1)]);
        runFrame();
        expect(wordingOf('req-1')).toBeUndefined();

        const listener = vi.fn();
        store.subscribe(listener);
        store.noteCallWording(BOT, 'req-1', '  消息内容为空 ');
        expect(wordingOf('req-1')).toBe('消息内容为空');
        expect(listener).toHaveBeenCalledTimes(1);

        // 同一句再来一次：什么都不变
        const snapshot = store.getSnapshot();
        store.noteCallWording(BOT, 'req-1', '消息内容为空');
        expect(store.getSnapshot()).toBe(snapshot);
    });

    it('回包先到：先记着不动 store，事件 reduce 时补上', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([groupMessage(1)]);
        runFrame();

        const snapshot = store.getSnapshot();
        store.noteCallWording(BOT, 'req-1', '消息内容为空');
        expect(store.getSnapshot()).toBe(snapshot);

        wires[0].push([failedSend(2)]);
        runFrame();
        expect(wordingOf('req-1')).toBe('消息内容为空');
    });

    it('事件已经到了但还在队列里等下一帧：回包这时候来也补得上', async () => {
        const wires = installSubscribe();
        await store.ensureReceiving(BOT, AUTO, SELF);
        wires[0].push([failedSend(1)]);
        store.noteCallWording(BOT, 'req-1', '消息内容为空');
        runFrame();
        expect(wordingOf('req-1')).toBe('消息内容为空');
    });

    it('还没订阅过这个 Bot 时先记着，订上之后补来的积压里认得出', async () => {
        store.noteCallWording(BOT, 'req-1', '消息内容为空');
        installSubscribe({ backlog: [failedSend(1)], receiver: info({ buffered: 1 }) });
        await store.ensureReceiving(BOT, AUTO, SELF);
        runFrame();
        expect(wordingOf('req-1')).toBe('消息内容为空');
    });
});

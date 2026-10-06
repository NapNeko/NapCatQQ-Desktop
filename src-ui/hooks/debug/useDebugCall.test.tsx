import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugCallRequest } from '../../core/ipc/generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';

const callMock = vi.fn();
const cancelMock = vi.fn();
const pushErrorBar = vi.fn();

vi.mock('../../core/services/onebot-debug.service', () => ({
    onebotDebugService: {
        call: (...args: unknown[]) => callMock(...args),
        cancel: (...args: unknown[]) => cancelMock(...args),
        workspace: vi.fn(),
        saveWorkspace: vi.fn(),
    },
}));

vi.mock('../ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBar(...args),
}));

import { useDebugCall } from './useDebugCall';
import { debugEventStore } from './debugEventStore';
import { debugWorkspaceStore } from './debugWorkspaceStore';

type Req = Omit<DebugCallRequest, 'request_id'>;

const req = (action = 'get_login_info', over: Partial<Req> = {}): Req => ({
    bot_id: 'bot-1',
    channel: { kind: 'auto' },
    action,
    params: {},
    timeout_ms: null,
    origin: 'editor',
    ...over,
});

const okResponse = (
    requestId: string,
    retcode = 0,
    text: { message?: string; wording?: string } = {},
): DebugCallResponse => ({
    request_id: requestId,
    result: {
        kind: 'ok',
        outcome: {
            ok: retcode === 0,
            status: retcode === 0 ? 'ok' : 'failed',
            retcode,
            data: null,
            message: text.message ?? '',
            wording: text.wording ?? '',
            raw: null,
            elapsed_ms: 12,
            channel: { kind: 'internal' },
            size_bytes: 2,
            truncated: false,
        },
    },
});

function deferred<T>() {
    let resolve!: (v: T) => void;
    let reject!: (e: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function mount() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    let renders = 0;
    const hook = renderHook(
        () => {
            renders += 1;
            return useDebugCall();
        },
        { wrapper },
    );
    return { ...hook, invalidate, renders: () => renders };
}

const requestIdOf = (call: number): string =>
    (callMock.mock.calls[call][0] as DebugCallRequest).request_id;

beforeEach(() => {
    callMock.mockReset();
    cancelMock.mockReset();
    cancelMock.mockResolvedValue(undefined);
    pushErrorBar.mockReset();
    debugWorkspaceStore._reset();
});

afterEach(() => {
    // 先卸载再重置 store，否则重置的通知会在组件还挂着时触发一次没包 act 的更新
    cleanup();
    debugWorkspaceStore._reset();
    debugEventStore._reset();
    vi.restoreAllMocks();
});

describe('useDebugCall.send', () => {
    it('发出时记 inflight，回来后写 last 并清掉 inflight，历史失效', async () => {
        const gate = deferred<DebugCallResponse>();
        callMock.mockReturnValue(gate.promise);
        const { result, invalidate } = mount();

        let sending!: Promise<DebugCallResponse>;
        act(() => {
            sending = result.current.send('tab-1', req());
        });

        expect(result.current.isInflight('tab-1')).toBe(true);
        expect(result.current.isInflight('tab-2')).toBe(false);
        expect(debugWorkspaceStore.getRun('tab-1')?.inflight?.requestId).toBe(requestIdOf(0));
        expect(callMock.mock.calls[0][0]).toMatchObject({
            bot_id: 'bot-1',
            action: 'get_login_info',
            origin: 'editor',
        });

        const response = okResponse(requestIdOf(0));
        await act(async () => {
            gate.resolve(response);
            await sending;
        });

        const run = debugWorkspaceStore.getRun('tab-1');
        expect(run?.inflight).toBeUndefined();
        expect(run?.last).toMatchObject({ response, botId: 'bot-1', action: 'get_login_info' });
        expect(result.current.isInflight('tab-1')).toBe(false);
        // 历史查询键都以这个前缀开头，前缀失效能覆盖到所有筛选条件
        expect(invalidate).toHaveBeenCalledWith({ queryKey: ['debug', 'history'] });
        // 每次调用后端都会把通道状态记回会话：通道列表（含「自动」落点）要跟着重拉
        expect(invalidate).toHaveBeenCalledWith({ queryKey: ['debug', 'channels', 'bot-1'] });
    });

    it('同一标签连发：先发的后回来也不覆盖，只显示最后一次；两次都进历史', async () => {
        const first = deferred<DebugCallResponse>();
        const second = deferred<DebugCallResponse>();
        callMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
        const { result, invalidate } = mount();

        let p1!: Promise<DebugCallResponse>;
        let p2!: Promise<DebugCallResponse>;
        act(() => {
            p1 = result.current.send('tab-1', req('get_group_list'));
            p2 = result.current.send('tab-1', req('get_friend_list'));
        });
        const id1 = requestIdOf(0);
        const id2 = requestIdOf(1);
        expect(id1).not.toBe(id2);
        expect(debugWorkspaceStore.getRun('tab-1')?.inflight?.requestId).toBe(id2);

        // 后发的先回
        await act(async () => {
            second.resolve(okResponse(id2));
            await p2;
        });
        expect(debugWorkspaceStore.getRun('tab-1')?.last?.response.request_id).toBe(id2);

        // 先发的晚到：返回值照给调用方，但不覆盖显示
        let late!: DebugCallResponse;
        await act(async () => {
            first.resolve(okResponse(id1, 1400));
            late = await p1;
        });
        expect(late.request_id).toBe(id1);
        expect(debugWorkspaceStore.getRun('tab-1')?.last?.response.request_id).toBe(id2);
        expect(debugWorkspaceStore.getRun('tab-1')?.last?.action).toBe('get_friend_list');
        // 每次发送各失效两次：历史 + 通道
        expect(invalidate).toHaveBeenCalledTimes(4);
    });

    it('先发的先回：inflight 还是后发的那次，先不写结果', async () => {
        const first = deferred<DebugCallResponse>();
        const second = deferred<DebugCallResponse>();
        callMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
        const { result } = mount();

        let p1!: Promise<DebugCallResponse>;
        let p2!: Promise<DebugCallResponse>;
        act(() => {
            p1 = result.current.send('tab-1', req('a'));
            p2 = result.current.send('tab-1', req('b'));
        });

        await act(async () => {
            first.resolve(okResponse(requestIdOf(0)));
            await p1;
        });
        expect(debugWorkspaceStore.getRun('tab-1')?.last).toBeUndefined();
        expect(result.current.isInflight('tab-1')).toBe(true);

        await act(async () => {
            second.resolve(okResponse(requestIdOf(1)));
            await p2;
        });
        expect(debugWorkspaceStore.getRun('tab-1')?.last?.action).toBe('b');
        expect(result.current.isInflight('tab-1')).toBe(false);
    });

    it('等的时候标签被关了 / 换了动作：结果只进历史，不再冒出来', async () => {
        const gate = deferred<DebugCallResponse>();
        callMock.mockReturnValue(gate.promise);
        const { result } = mount();

        let sending!: Promise<DebugCallResponse>;
        act(() => {
            sending = result.current.send('tab-1', req());
        });
        act(() => debugWorkspaceStore.setRun('tab-1', null));

        await act(async () => {
            gate.resolve(okResponse(requestIdOf(0)));
            await sending;
        });

        expect(debugWorkspaceStore.getRun('tab-1')).toBeUndefined();
    });

    it('tabId 为 null（聊天输入框、选择器）：只返回结果，不碰任何标签', async () => {
        callMock.mockImplementation(async (r: DebugCallRequest) => okResponse(r.request_id));
        const { result, invalidate } = mount();

        let response!: DebugCallResponse;
        await act(async () => {
            response = await result.current.send(
                null,
                req('send_group_msg', { origin: 'composer' }),
            );
        });

        expect(response.request_id).toBe(requestIdOf(0));
        expect(debugWorkspaceStore.getSnapshot().runs).toEqual({});
        expect(callMock.mock.calls[0][0]).toMatchObject({ origin: 'composer' });
        expect(invalidate).toHaveBeenCalled();
    });

    it('invoke 自己抛错：折成 internal 错误的回包，同样写进 last，不 reject', async () => {
        callMock.mockRejectedValue('IPC 断了');
        const { result } = mount();

        let response!: DebugCallResponse;
        await act(async () => {
            response = await result.current.send('tab-1', req());
        });

        expect(response.result).toEqual({
            kind: 'err',
            error: { kind: 'internal', message: 'IPC 断了' },
        });
        expect(debugWorkspaceStore.getRun('tab-1')?.last?.response).toEqual(response);
        expect(debugWorkspaceStore.getRun('tab-1')?.inflight).toBeUndefined();
    });

    it('新一次调用发出去时保留上一次的结果', async () => {
        callMock.mockImplementation(async (r: DebugCallRequest) => okResponse(r.request_id));
        const { result } = mount();
        await act(async () => {
            await result.current.send('tab-1', req('a'));
        });

        const gate = deferred<DebugCallResponse>();
        callMock.mockReturnValue(gate.promise);
        act(() => {
            void result.current.send('tab-1', req('b'));
        });

        expect(debugWorkspaceStore.getRun('tab-1')?.last?.action).toBe('a');
        expect(debugWorkspaceStore.getRun('tab-1')?.inflight).toBeDefined();
    });
});

describe('useDebugCall · 不跟着 runs 重渲染', () => {
    it('任何标签的调用起落都不让挂着它的组件重画；isInflight 读的是当下的快照', async () => {
        const gate = deferred<DebugCallResponse>();
        callMock.mockReturnValue(gate.promise);
        const { result, renders } = mount();
        const api = result.current;
        const before = renders();

        let sending!: Promise<DebugCallResponse>;
        act(() => {
            sending = result.current.send('tab-1', req());
            debugWorkspaceStore.setRun('tab-2', { inflight: { requestId: 'other', startedAt: 1 } });
        });
        expect(result.current.isInflight('tab-1')).toBe(true);
        expect(result.current.isInflight('tab-2')).toBe(true);

        await act(async () => {
            gate.resolve(okResponse(requestIdOf(0)));
            await sending;
        });
        expect(result.current.isInflight('tab-1')).toBe(false);
        expect(renders()).toBe(before);
        // 返回的对象和函数引用都不变，放进依赖数组也不会白跑 effect
        expect(result.current).toBe(api);
    });
});

describe('useDebugCall · 失败说明交给聊天时间线', () => {
    it('拿到回包但 OB11 失败：把 wording（没有就 message）按 bot + request_id 交给事件 store', async () => {
        const note = vi.spyOn(debugEventStore, 'noteCallWording');
        callMock
            .mockImplementationOnce(async (r: DebugCallRequest) =>
                okResponse(r.request_id, 1200, { wording: '消息内容为空', message: 'EMPTY' }),
            )
            .mockImplementationOnce(async (r: DebugCallRequest) =>
                okResponse(r.request_id, 1404, { message: ' group not found ' }),
            );
        const { result } = mount();

        await act(async () => {
            await result.current.send(null, req('send_group_msg', { origin: 'composer' }));
            await result.current.send('tab-1', req('get_group_info'));
        });

        expect(note.mock.calls).toEqual([
            ['bot-1', requestIdOf(0), '消息内容为空'],
            ['bot-1', requestIdOf(1), 'group not found'],
        ]);
    });

    it('成功、没拿到回包、选择器的查询、上游什么都没说：都不记', async () => {
        const note = vi.spyOn(debugEventStore, 'noteCallWording');
        callMock
            .mockImplementationOnce(async (r: DebugCallRequest) => okResponse(r.request_id))
            .mockImplementationOnce(async (r: DebugCallRequest) => ({
                request_id: r.request_id,
                result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } },
            }))
            .mockImplementationOnce(async (r: DebugCallRequest) =>
                okResponse(r.request_id, 1200, { wording: '拉不到' }),
            )
            .mockImplementationOnce(async (r: DebugCallRequest) => okResponse(r.request_id, 1200));
        const { result } = mount();

        await act(async () => {
            await result.current.send(null, req('send_group_msg', { origin: 'composer' }));
            await result.current.send(null, req('send_group_msg', { origin: 'composer' }));
            await result.current.send(null, req('get_group_member_list', { origin: 'picker' }));
            await result.current.send(null, req('send_group_msg', { origin: 'composer' }));
        });

        expect(note).not.toHaveBeenCalled();
    });
});

describe('useDebugCall.cancel', () => {
    it('按 inflight 的 requestId 取消；没有在等的调用就什么也不做', async () => {
        const gate = deferred<DebugCallResponse>();
        callMock.mockReturnValue(gate.promise);
        const { result } = mount();

        await act(async () => {
            await result.current.cancel('tab-1');
        });
        expect(cancelMock).not.toHaveBeenCalled();

        act(() => {
            void result.current.send('tab-1', req());
        });
        await act(async () => {
            await result.current.cancel('tab-1');
        });
        expect(cancelMock).toHaveBeenCalledWith(requestIdOf(0));
    });

    it('取消失败弹错误条', async () => {
        callMock.mockReturnValue(new Promise(() => {}));
        cancelMock.mockRejectedValue('取消不了');
        const { result } = mount();
        act(() => {
            void result.current.send('tab-1', req());
        });

        await act(async () => {
            await result.current.cancel('tab-1');
        });

        expect(pushErrorBar).toHaveBeenCalledWith(expect.objectContaining({ key: 'debug-cancel' }));
    });
});

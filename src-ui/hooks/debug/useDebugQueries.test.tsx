import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DomainEvent } from '../../core/ipc/types';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugCatalog } from '../../core/ipc/generated/debug/DebugCatalog';
import type { DebugChannels } from '../../core/ipc/generated/debug/DebugChannels';
import type { DebugCollections } from '../../core/ipc/generated/debug/DebugCollections';

const service = {
    targets: vi.fn(),
    channels: vi.fn(),
    testChannel: vi.fn(),
    catalog: vi.fn(),
    describe: vi.fn(),
    call: vi.fn(),
    collections: vi.fn(),
    saveCollections: vi.fn(),
    pickCollectionsFile: vi.fn(),
    importCollections: vi.fn(),
    workspace: vi.fn(),
    saveWorkspace: vi.fn(),
};
const pushErrorBar = vi.fn();
const pushInfoBar = vi.fn();
let domainHandlers: Array<(event: DomainEvent) => void> = [];
const subscribeDomainEvents = vi.fn((handler: (event: DomainEvent) => void) => {
    domainHandlers.push(handler);
    return () => {
        domainHandlers = domainHandlers.filter((h) => h !== handler);
    };
});

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
    subscribeDomainEvents: (handler: (event: DomainEvent) => void) =>
        subscribeDomainEvents(handler),
}));
vi.mock('../ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBar(...args),
}));
vi.mock('../ui/globalInfoBarStore', () => ({
    pushInfoBar: (...args: unknown[]) => pushInfoBar(...args),
}));

import { useDebugTargets } from './useDebugTargets';
import { useDebugChannels, useTestChannel } from './useDebugChannels';
import { _resetDebugCatalogForTests, useDebugCatalog } from './useDebugCatalog';
import { useDebugContacts } from './useDebugContacts';
import {
    useDebugCollections,
    useImportCollections,
    useSaveCollections,
} from './useDebugCollections';
import { debugWorkspaceStore } from './debugWorkspaceStore';
import { debugCatalogPrefix, debugChannelsKey, debugSpecPrefix, debugTargetsKey } from './keys';

function makeClient() {
    return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function wrapperFor(client: QueryClient) {
    return ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
}

const fire = (event: DomainEvent) => act(() => domainHandlers.forEach((h) => h(event)));

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    service.targets.mockResolvedValue([]);
    pushErrorBar.mockReset();
    pushInfoBar.mockReset();
    subscribeDomainEvents.mockClear();
    domainHandlers = [];
    _resetDebugCatalogForTests();
    debugWorkspaceStore._reset();
});

afterEach(() => {
    cleanup();
    debugWorkspaceStore._reset();
});

describe('useDebugTargets', () => {
    it('多个组件同时用只订一份事件；最后一个卸载才退订', async () => {
        const client = makeClient();
        const wrapper = wrapperFor(client);
        const a = renderHook(() => useDebugTargets(), { wrapper });
        const b = renderHook(() => useDebugTargets(), { wrapper });
        await waitFor(() => expect(a.result.current.isSuccess).toBe(true));

        expect(subscribeDomainEvents).toHaveBeenCalledTimes(1);
        a.unmount();
        expect(domainHandlers).toHaveLength(1);
        b.unmount();
        expect(domainHandlers).toHaveLength(0);
    });

    it('Bot 状态 / WebUI / SnowLuma 登录事件让对应的缓存失效', async () => {
        const client = makeClient();
        const invalidate = vi.spyOn(client, 'invalidateQueries');
        const { result } = renderHook(() => useDebugTargets(), { wrapper: wrapperFor(client) });
        await waitFor(() => expect(result.current.isSuccess).toBe(true));
        invalidate.mockClear();

        fire({ kind: 'bot_state_changed', snapshot: { bot_id: 'b1' } } as DomainEvent);
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugTargetsKey });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugChannelsKey('b1') });
        // 目录也可能因此换源（内部通道就绪 / 不再就绪），快照不能一直盖着在线版
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugCatalogPrefix('b1') });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugSpecPrefix('b1') });

        invalidate.mockClear();
        fire({
            kind: 'napcat_webui_available',
            bot_id: 'b2',
            port: 6099,
            token: 't',
        } as DomainEvent);
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugChannelsKey('b2') });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugCatalogPrefix('b2') });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugSpecPrefix('b2') });
        expect(invalidate).not.toHaveBeenCalledWith({ queryKey: debugTargetsKey });

        invalidate.mockClear();
        fire({ kind: 'snowluma_uin_detected', bot_id: 'b3', uin: '1' } as DomainEvent);
        fire({
            kind: 'snowluma_login_state_changed',
            bot_id: 'b4',
            state: 'LoggedIn',
        } as unknown as DomainEvent);
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugChannelsKey('b3') });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugChannelsKey('b4') });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugCatalogPrefix('b3') });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugCatalogPrefix('b4') });

        // NapCat 登录状态变了：列表里的「在线」标记要跟着刷新；上线时目录也该重取，被踢下线不影响 WebUI 的目录
        invalidate.mockClear();
        fire({ kind: 'napcat_login_online', bot_id: 'b5', online: true } as DomainEvent);
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugTargetsKey });
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugCatalogPrefix('b5') });
        invalidate.mockClear();
        fire({
            kind: 'napcat_login_invalidated',
            bot_id: 'b5',
            reason: 'KickedOffline',
        } as unknown as DomainEvent);
        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugTargetsKey });
        expect(invalidate).not.toHaveBeenCalledWith({ queryKey: debugCatalogPrefix('b5') });

        invalidate.mockClear();
        fire({ kind: 'bot_log_appended', bot_id: 'b1', line: 'x' } as DomainEvent);
        expect(invalidate).not.toHaveBeenCalled();
    });

    it('读取失败弹错误条', async () => {
        service.targets.mockRejectedValue('后端没起来');
        renderHook(() => useDebugTargets(), { wrapper: wrapperFor(makeClient()) });
        await waitFor(() =>
            expect(pushErrorBar).toHaveBeenCalledWith(
                expect.objectContaining({ key: 'debug-targets' }),
            ),
        );
    });
});

describe('useTestChannel', () => {
    it('测完先把结果写回该条通道，并整串重拉（「自动」落点和别的通道也可能跟着变）', async () => {
        const client = makeClient();
        const before: DebugChannels = {
            bot_id: 'b1',
            auto_call: { kind: 'internal' },
            auto_events: { kind: 'internal' },
            channels: [
                {
                    id: { kind: 'internal' },
                    label: '内部',
                    can_call: true,
                    can_receive: true,
                    status: { kind: 'unknown' },
                    endpoint: null,
                    token_hint: null,
                },
                {
                    id: { kind: 'http', name: 'main' },
                    label: 'HTTP',
                    can_call: true,
                    can_receive: false,
                    status: { kind: 'unknown' },
                    endpoint: null,
                    token_hint: null,
                },
            ],
        };
        const after: DebugChannels = {
            ...before,
            channels: [
                before.channels[0],
                { ...before.channels[1], status: { kind: 'available' } },
            ],
        };
        service.channels.mockResolvedValueOnce(before).mockResolvedValue(after);
        const wrapper = wrapperFor(client);
        const list = renderHook(() => useDebugChannels('b1'), { wrapper });
        await waitFor(() => expect(list.result.current.isSuccess).toBe(true));

        const invalidate = vi.spyOn(client, 'invalidateQueries');
        service.testChannel.mockResolvedValue(after.channels[1]);
        const test = renderHook(() => useTestChannel(), { wrapper });
        await act(async () => {
            await test.result.current.mutateAsync({
                botId: 'b1',
                channel: { kind: 'http', name: 'main' },
            });
        });

        expect(invalidate).toHaveBeenCalledWith({ queryKey: debugChannelsKey('b1') });
        await waitFor(() =>
            expect(
                client.getQueryData<DebugChannels>(debugChannelsKey('b1'))?.channels[1].status,
            ).toEqual({ kind: 'available' }),
        );
        expect(
            client.getQueryData<DebugChannels>(debugChannelsKey('b1'))?.channels[0].status,
        ).toEqual({ kind: 'unknown' });
        expect(service.channels).toHaveBeenCalledTimes(2);
    });
});

describe('useDebugCatalog', () => {
    const catalog = (source: 'live' | 'snapshot'): DebugCatalog => ({
        backend: 'napcat',
        source,
        snapshot_version: 'x',
        actions: [],
    });

    it('没有目标时不请求', () => {
        renderHook(() => useDebugCatalog(null), { wrapper: wrapperFor(makeClient()) });
        expect(service.catalog).not.toHaveBeenCalled();
    });

    it('Bot 的运行状态翻转时重取；没翻转时用缓存', async () => {
        service.catalog
            .mockResolvedValueOnce(catalog('snapshot'))
            .mockResolvedValueOnce(catalog('live'));
        const client = makeClient();
        const { result, rerender } = renderHook(
            (p: { running: boolean }) =>
                useDebugCatalog({ bot_id: 'b1', backend: 'napcat', running: p.running }),
            { wrapper: wrapperFor(client), initialProps: { running: false } },
        );
        await waitFor(() => expect(result.current.data?.source).toBe('snapshot'));
        expect(service.catalog).toHaveBeenCalledWith('b1', 'napcat');

        rerender({ running: false });
        expect(service.catalog).toHaveBeenCalledTimes(1);

        rerender({ running: true });
        await waitFor(() => expect(result.current.data?.source).toBe('live'));
        expect(service.catalog).toHaveBeenCalledTimes(2);
    });

    it('没选 Bot 时按后端看内置快照', async () => {
        service.catalog.mockResolvedValue(catalog('snapshot'));
        const { result } = renderHook(
            () => useDebugCatalog({ bot_id: null, backend: 'snowluma' }),
            {
                wrapper: wrapperFor(makeClient()),
            },
        );
        await waitFor(() => expect(result.current.isSuccess).toBe(true));
        expect(service.catalog).toHaveBeenCalledWith(null, 'snowluma');
    });

    it('挂着快照且 Bot 在跑：过一阵自动再问一次，换到在线版后停下', async () => {
        vi.useFakeTimers();
        try {
            service.catalog
                .mockResolvedValueOnce(catalog('snapshot'))
                .mockResolvedValueOnce(catalog('live'))
                .mockResolvedValue(catalog('live'));
            const client = makeClient();
            const { result } = renderHook(
                () => useDebugCatalog({ bot_id: 'b1', backend: 'napcat', running: true }),
                {
                    wrapper: wrapperFor(client),
                },
            );
            await vi.waitFor(() => expect(result.current.data?.source).toBe('snapshot'));
            expect(service.catalog).toHaveBeenCalledTimes(1);

            // 还没到下一分钟到不了第二次；过一分钟自动再问，数据换成在线版
            await vi.advanceTimersByTimeAsync(59_000);
            expect(service.catalog).toHaveBeenCalledTimes(1);
            await vi.waitFor(() => expect(result.current.data?.source).toBe('live'), {
                timeout: 90_000,
            });
            expect(service.catalog).toHaveBeenCalledTimes(2);

            // 换到在线版之后不再定时重拉
            await vi.advanceTimersByTimeAsync(240_000);
            expect(service.catalog).toHaveBeenCalledTimes(2);
        } finally {
            vi.useRealTimers();
        }
    });

    it('挂着快照但 Bot 没在跑：不定时重拉', async () => {
        vi.useFakeTimers();
        try {
            service.catalog.mockResolvedValue(catalog('snapshot'));
            renderHook(() => useDebugCatalog({ bot_id: 'b1', backend: 'napcat', running: false }), {
                wrapper: wrapperFor(makeClient()),
            });
            await act(async () => {
                await vi.advanceTimersByTimeAsync(0);
            });
            expect(service.catalog).toHaveBeenCalledTimes(1);
            await act(async () => {
                await vi.advanceTimersByTimeAsync(180_000);
            });
            expect(service.catalog).toHaveBeenCalledTimes(1);
        } finally {
            vi.useRealTimers();
        }
    });
});

describe('useDebugContacts', () => {
    const okData = (data: unknown): DebugCallResponse => ({
        request_id: 'r',
        result: {
            kind: 'ok',
            outcome: {
                ok: true,
                status: 'ok',
                retcode: 0,
                data,
                message: '',
                wording: '',
                raw: null,
                elapsed_ms: 1,
                channel: { kind: 'internal' },
                size_bytes: 1,
                truncated: false,
            },
        },
    });

    it('群列表：按当前调用通道发 picker 调用，转成选项', async () => {
        debugWorkspaceStore.setChannelChoice('b1', {
            call: { kind: 'http', name: 'main' },
            events: { kind: 'auto' },
        });
        service.call.mockResolvedValue(
            okData([
                { group_id: 100, group_name: '测试群', member_count: 30 },
                { group_id: '200', group_name: '' },
                { group_name: '没有群号的行' },
            ]),
        );
        const { result } = renderHook(
            () => useDebugContacts({ bot_id: 'b1', running: true }, 'group'),
            {
                wrapper: wrapperFor(makeClient()),
            },
        );

        await waitFor(() => expect(result.current.options).toHaveLength(2));
        expect(result.current.options[0]).toEqual({
            id: 100,
            label: '测试群',
            hint: '100 · 30 人',
        });
        expect(result.current.options[1]).toEqual({ id: 200, label: '200', hint: '200' });
        expect(result.current.error).toBeNull();
        expect(service.call).toHaveBeenCalledWith(
            expect.objectContaining({
                bot_id: 'b1',
                action: 'get_group_list',
                origin: 'picker',
                timeout_ms: 15000,
                channel: { kind: 'http', name: 'main' },
            }),
        );
    });

    it('成员列表要等有群号才请求，名片优先，管理员带标注', async () => {
        service.call.mockResolvedValue(
            okData([
                { user_id: 1, nickname: '昵称', card: '名片', role: 'admin' },
                { user_id: 2, nickname: '路人', card: '', role: 'member' },
            ]),
        );
        const client = makeClient();
        const { result, rerender } = renderHook(
            (p: { groupId: string }) =>
                useDebugContacts({ bot_id: 'b1', running: true }, 'member', p.groupId),
            { wrapper: wrapperFor(client), initialProps: { groupId: '' } },
        );
        expect(service.call).not.toHaveBeenCalled();
        expect(result.current.options).toEqual([]);

        rerender({ groupId: '100' });
        await waitFor(() => expect(result.current.options).toHaveLength(2));
        expect(service.call.mock.calls[0][0]).toMatchObject({
            action: 'get_group_member_list',
            params: { group_id: 100 },
        });
        expect(result.current.options[0]).toEqual({
            id: 1,
            label: '名片',
            hint: '昵称 · 1 · 管理员',
        });
        expect(result.current.options[1]).toEqual({ id: 2, label: '路人', hint: '2' });
    });

    it('拉取失败只给 error，不弹错误条（选择器退化成输入框）', async () => {
        service.call.mockResolvedValue({
            request_id: 'r',
            result: { kind: 'err', error: { kind: 'timeout', ms: 15000 } },
        } satisfies DebugCallResponse);
        const { result } = renderHook(
            () => useDebugContacts({ bot_id: 'b1', running: true }, 'friend'),
            {
                wrapper: wrapperFor(makeClient()),
            },
        );

        await waitFor(() => expect(result.current.error).not.toBeNull());
        expect(result.current.options).toEqual([]);
        expect(pushErrorBar).not.toHaveBeenCalled();
    });

    it('Bot 没运行时不请求', () => {
        renderHook(() => useDebugContacts({ bot_id: 'b1', running: false }, 'group'), {
            wrapper: wrapperFor(makeClient()),
        });
        expect(service.call).not.toHaveBeenCalled();
    });
});

describe('useSaveCollections', () => {
    const base: DebugCollections = { version: 1, folders: [], requests: [] };
    const folder = (id: string, order: number) => ({ id, name: id, order });
    const one: DebugCollections = { ...base, folders: [folder('f1', 0)] };
    const two: DebugCollections = { ...base, folders: [folder('f1', 0), folder('f2', 1)] };

    // 在渲染里就把要看的字段读出来：react-query 只在被读过的字段变化时才让组件重渲染。
    // folderCounts 记下每次渲染看到的文件夹数，用来确认界面有没有被拉回旧值
    const readCollections = (
        wrapper: ReturnType<typeof wrapperFor>,
        folderCounts: Array<number | null> = [],
    ) =>
        renderHook(
            () => {
                const { data, isSuccess } = useDebugCollections();
                folderCounts.push(data ? data.folders.length : null);
                return { data, isSuccess };
            },
            { wrapper },
        );

    it('先乐观更新缓存；单次写失败：退回去、弹错误条，并重拉磁盘内容', async () => {
        service.collections.mockResolvedValue(base);
        let rejectSave!: (e: unknown) => void;
        service.saveCollections.mockReturnValue(new Promise((_, rej) => (rejectSave = rej)));
        const wrapper = wrapperFor(makeClient());
        const read = readCollections(wrapper);
        await waitFor(() => expect(read.result.current.isSuccess).toBe(true));
        const save = renderHook(() => useSaveCollections(), { wrapper });

        act(() => save.result.current.mutate(one));
        await waitFor(() => expect(read.result.current.data).toEqual(one));

        await act(async () => {
            rejectSave('磁盘满了');
        });
        await waitFor(() =>
            expect(pushErrorBar).toHaveBeenCalledWith(
                expect.objectContaining({ key: 'debug-collections-save' }),
            ),
        );
        await waitFor(() => expect(read.result.current.data).toEqual(base));
        await waitFor(() => expect(service.collections).toHaveBeenCalledTimes(2));
    });

    it('写成功：缓存保持新值，结束后重拉一次确认磁盘内容', async () => {
        service.collections.mockResolvedValueOnce(base).mockResolvedValue(one);
        service.saveCollections.mockResolvedValue(undefined);
        const wrapper = wrapperFor(makeClient());
        const read = readCollections(wrapper);
        await waitFor(() => expect(read.result.current.isSuccess).toBe(true));
        const save = renderHook(() => useSaveCollections(), { wrapper });

        act(() => save.result.current.mutate(one));

        await waitFor(() => expect(read.result.current.data).toEqual(one));
        expect(service.saveCollections).toHaveBeenCalledWith(one);
        await waitFor(() => expect(service.collections).toHaveBeenCalledTimes(2));
    });

    it('先失败后成功的连续保存：失败的那次不会把后一次的乐观值盖回旧的，也不会在中途重拉', async () => {
        // 第二次保存写完之后，磁盘上就是 two
        service.collections.mockResolvedValueOnce(base).mockResolvedValue(two);
        let rejectFirst!: (e: unknown) => void;
        service.saveCollections
            .mockReturnValueOnce(new Promise((_, rej) => (rejectFirst = rej)))
            .mockResolvedValueOnce(undefined);
        const folderCounts: Array<number | null> = [];
        const wrapper = wrapperFor(makeClient());
        const read = readCollections(wrapper, folderCounts);
        await waitFor(() => expect(read.result.current.isSuccess).toBe(true));
        const save = renderHook(() => useSaveCollections(), { wrapper });

        act(() => {
            save.result.current.mutate(one);
            save.result.current.mutate(two);
        });
        await waitFor(() => expect(read.result.current.data).toEqual(two));

        // 第一次失败：缓存里已经是第二次的值，不许退回去；第二次还排着队，这时也不该重拉
        await act(async () => {
            rejectFirst('磁盘满了');
        });
        await waitFor(() => expect(pushErrorBar).toHaveBeenCalledTimes(1));
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(2));
        expect(service.saveCollections).toHaveBeenNthCalledWith(2, two);

        // 第二次写完，是最后一个结束的保存：这时才重拉一次
        await waitFor(() => expect(service.collections).toHaveBeenCalledTimes(2));
        await waitFor(() => expect(read.result.current.data).toEqual(two));
        // 界面从头到尾没回到过旧值：看到 2 个文件夹之后再没变小过
        const afterTwo = folderCounts.slice(folderCounts.indexOf(2));
        expect(afterTwo.every((n) => n === 2)).toBe(true);
    });
});

describe('useImportCollections', () => {
    const merged: DebugCollections = { version: 1, folders: [], requests: [] };

    it('对话框里多选了几个文件：只导第一个，并用警告条说明', async () => {
        service.pickCollectionsFile.mockResolvedValue({ path: 'C:/a.json', ignored: 2 });
        service.importCollections.mockResolvedValue(merged);
        service.collections.mockResolvedValue(merged);
        const { result } = renderHook(() => useImportCollections(), {
            wrapper: wrapperFor(makeClient()),
        });

        await act(async () => {
            await result.current.mutateAsync();
        });

        expect(service.importCollections).toHaveBeenCalledTimes(1);
        expect(service.importCollections).toHaveBeenCalledWith('C:/a.json');
        expect(pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({ key: 'debug-import-multi', tone: 'warning' }),
        );
        expect(pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({ key: 'debug-collections-import' }),
        );
    });

    it('只选了一个文件不提示；取消选择什么也不做', async () => {
        service.importCollections.mockResolvedValue(merged);
        const { result } = renderHook(() => useImportCollections(), {
            wrapper: wrapperFor(makeClient()),
        });

        service.pickCollectionsFile.mockResolvedValue({ path: 'C:/a.json', ignored: 0 });
        await act(async () => {
            await result.current.mutateAsync();
        });
        expect(pushInfoBar).not.toHaveBeenCalledWith(
            expect.objectContaining({ key: 'debug-import-multi' }),
        );

        service.importCollections.mockClear();
        pushInfoBar.mockClear();
        service.pickCollectionsFile.mockResolvedValue(null);
        await act(async () => {
            await result.current.mutateAsync();
        });
        expect(service.importCollections).not.toHaveBeenCalled();
        expect(pushInfoBar).not.toHaveBeenCalled();
    });
});

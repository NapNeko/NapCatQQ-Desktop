import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const subscribeMock = vi.fn();

vi.mock('./event-stream.service', () => ({
    eventStreamService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
    },
}));

vi.mock('./bot.service', () => ({
    botService: {
        listNapcatWebuiBindings: vi.fn(async () => []),
        listSnowlumaUiSnapshot: vi.fn(async () => ({
            daemon_state: null,
            daemon_states: {},
            bots: [],
        })),
    },
}));

import {
    _domainEventHandlerCountForTests,
    _resetDomainEventHubForTests,
    subscribeDomainEvents,
} from './domain-event-hub';
import { napcatLoginStore } from '../../hooks/webui/napcatLoginStore';
import { snowlumaStore } from '../../hooks/webui/snowlumaStore';
import type { DomainEvent } from '../ipc/types';

type StreamHandler = (event: DomainEvent) => void;

function installSubscribeMock(): {
    getHandler: () => StreamHandler | null;
    callCount: () => number;
} {
    let streamHandler: StreamHandler | null = null;
    let calls = 0;
    subscribeMock.mockImplementation(async (cb: StreamHandler) => {
        calls += 1;
        streamHandler = cb;
        return () => {
            if (streamHandler === cb) streamHandler = null;
        };
    });
    return {
        getHandler: () => streamHandler,
        callCount: () => calls,
    };
}

beforeEach(() => {
    subscribeMock.mockReset();
    _resetDomainEventHubForTests();
    napcatLoginStore._reset();
    snowlumaStore._reset();
});

afterEach(() => {
    napcatLoginStore._reset();
    snowlumaStore._reset();
    _resetDomainEventHubForTests();
});

describe('domain-event-hub 单次底层订阅', () => {
    it('多个 handler 与 login/snowluma store 只触发一次 eventStreamService.subscribe', async () => {
        const sub = installSubscribeMock();

        const seenA: string[] = [];
        const unsubA = subscribeDomainEvents((e) => {
            seenA.push(e.kind);
        });

        // 等 hub 异步 ensureStarted 完成
        await Promise.resolve();
        await Promise.resolve();

        expect(sub.callCount()).toBe(1);

        const unsubNapcat = napcatLoginStore.subscribe(() => { });
        const unsubSnow = snowlumaStore.subscribe(() => { });
        await Promise.resolve();
        await Promise.resolve();

        // store 只挂 hub handler，不再开第二套底层 listen
        expect(sub.callCount()).toBe(1);
        expect(_domainEventHandlerCountForTests()).toBeGreaterThanOrEqual(3);

        const handler = sub.getHandler();
        expect(handler).toBeTruthy();

        handler!({
            kind: 'snowluma_daemon_state_changed',
            state: 'ready',
            ref_count: 1,
            server_id: 'local',
        } as DomainEvent);

        expect(seenA).toContain('snowluma_daemon_state_changed');
        expect(snowlumaStore.getSnapshot().daemonStates.local).toBe('ready');

        unsubA();
        unsubNapcat();
        unsubSnow();
    });

    it('_reset 后可再次订阅且不叠两套底层 listen', async () => {
        const sub = installSubscribeMock();

        const unsub1 = napcatLoginStore.subscribe(() => { });
        await Promise.resolve();
        await Promise.resolve();
        expect(sub.callCount()).toBe(1);

        napcatLoginStore._reset();
        unsub1();
        // store _reset 已从 hub 卸 handler；若无其它 handler，底层会 teardown
        _resetDomainEventHubForTests();

        const unsub2 = napcatLoginStore.subscribe(() => { });
        await Promise.resolve();
        await Promise.resolve();
        // 重新 ensureStarted，仍是单次 subscribe（本轮）
        expect(sub.callCount()).toBe(2);

        unsub2();
    });
});

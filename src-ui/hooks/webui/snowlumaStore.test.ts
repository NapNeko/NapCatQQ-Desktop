// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DomainEvent } from '../../core/ipc/types';
import type { SnowlumaUiSnapshotDto } from '../../core/ipc/generated/domain/SnowlumaUiSnapshotDto';

const handlers = new Set<(event: DomainEvent) => void>();
const snapshotMock = vi.fn<() => Promise<SnowlumaUiSnapshotDto>>();

vi.mock('../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: (handler: (event: DomainEvent) => void) => {
        handlers.add(handler);
        return () => handlers.delete(handler);
    },
}));

vi.mock('../../core/services/bot.service', () => ({
    botService: {
        listSnowlumaUiSnapshot: () => snapshotMock(),
    },
}));

import { botSessionBadge } from '../../core/domain/bot/bot-status-presentation';
import { snowlumaStore } from './snowlumaStore';

const BOT = '10001';
let unsubscribe = () => {};

function snapshot(unavailable: boolean): SnowlumaUiSnapshotDto {
    return {
        daemon_state: null,
        daemon_states: { remote: 'ready' },
        bots: [
            {
                bot_id: BOT,
                injected: true,
                uin: BOT,
                login_state: unavailable ? null : 'logged_in',
                endpoints_ready: true,
                probe_unavailable: unavailable,
            },
        ],
    };
}

function emit(event: DomainEvent): void {
    for (const handler of handlers) handler(event);
}

function emitRunning(): void {
    emit({
        kind: 'bot_state_changed',
        snapshot: {
            bot_id: BOT,
            state: 'running',
            revision: 1,
            token_generation: 1,
            pending_restart: false,
        },
        reason: 'bootstrap_reconcile',
    });
}

async function flushHydrate(): Promise<void> {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
}

function sessionLabel(): string | undefined {
    const bot = snowlumaStore.getSnapshot().byBot[BOT];
    return botSessionBadge({
        flavor: 'snowluma',
        state: 'running',
        needsQrLogin: false,
        snowlumaDaemonState: 'ready',
        snowlumaLoginState: bot?.loginState,
        snowlumaProbeUnavailable: bot?.probeUnavailable,
        snowlumaDockerEndpointsReady: bot?.dockerEndpointsReady,
    })?.label;
}

beforeEach(() => {
    snowlumaStore._reset();
    handlers.clear();
    snapshotMock.mockReset();
});

afterEach(() => {
    unsubscribe();
    snowlumaStore._reset();
    handlers.clear();
});

describe('SnowLuma 登录探测快照恢复', () => {
    it('页面晚于探测失败订阅时仍显示 WebUI 不可用', async () => {
        snapshotMock.mockResolvedValue(snapshot(true));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(true);
        expect(sessionLabel()).toBe('WebUI 不可用');
    });

    it('重连接管的 hydrate 不把探测失败盖回探测登录', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(false));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();
        expect(sessionLabel()).toBe('QQ 已登录');

        emit({ kind: 'snowluma_login_probe_unavailable', bot_id: BOT });
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        emitRunning();
        await flushHydrate();

        expect(snapshotMock).toHaveBeenCalledTimes(2);
        expect(sessionLabel()).toBe('WebUI 不可用');
    });

    it('登录探测恢复后的快照清除失败标记', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();

        snapshotMock.mockResolvedValueOnce(snapshot(false));
        emitRunning();
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(false);
        expect(sessionLabel()).toBe('QQ 已登录');
    });

    it('迟到的失败快照不能覆盖已恢复的登录事件', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();

        let resolveSnapshot!: (value: SnowlumaUiSnapshotDto) => void;
        snapshotMock.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveSnapshot = resolve;
            }),
        );
        emitRunning();
        emit({ kind: 'snowluma_login_state_changed', bot_id: BOT, state: 'logged_in' });
        resolveSnapshot(snapshot(true));
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(false);
        expect(sessionLabel()).toBe('QQ 已登录');
    });

    it('hydrate 等待期间的日志事件不丢弃登录快照', async () => {
        let resolveSnapshot!: (value: SnowlumaUiSnapshotDto) => void;
        snapshotMock.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveSnapshot = resolve;
            }),
        );
        unsubscribe = snowlumaStore.subscribe(() => {});

        emit({ kind: 'bot_log_appended', bot_id: BOT, line: 'Bot 正在处理消息' });
        emit({ kind: 'snowluma_daemon_log', line: 'OneBot 已连接' });
        resolveSnapshot(snapshot(false));
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].loginState).toBe('logged_in');
        expect(sessionLabel()).toBe('QQ 已登录');
    });
});

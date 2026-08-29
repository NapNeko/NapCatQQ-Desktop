import { describe, expect, it } from 'vitest';
import { sortBotSnapshotsByCustomOrder, botSortStore } from './useBotSort';
import type { BotActorSnapshot } from '../../core/ipc/types';

describe('useBotSort', () => {
    const mockSnapshots: BotActorSnapshot[] = [
        {
            bot_id: '10001',
            state: 'running',
            pid: 1234,
            last_error: null,
            uptime_seconds: 100,
            auto_restart_count: 0,
            last_transition_time: '2026-08-29T00:00:00Z',
        },
        {
            bot_id: '10002',
            state: 'stopped',
            pid: null,
            last_error: null,
            uptime_seconds: 0,
            auto_restart_count: 0,
            last_transition_time: '2026-08-29T00:00:00Z',
        },
        {
            bot_id: '10003',
            state: 'crashed',
            pid: null,
            last_error: 'crash',
            uptime_seconds: 0,
            auto_restart_count: 0,
            last_transition_time: '2026-08-29T00:00:00Z',
        },
    ];

    it('sorts bots according to custom order array', () => {
        const order = ['10003', '10001', '10002'];
        const sorted = sortBotSnapshotsByCustomOrder(mockSnapshots, order);
        expect(sorted.map((s) => s.bot_id)).toEqual(['10003', '10001', '10002']);
    });

    it('places unlisted/new bots at the end in natural numeric order', () => {
        const order = ['10002'];
        const sorted = sortBotSnapshotsByCustomOrder(mockSnapshots, order);
        expect(sorted.map((s) => s.bot_id)).toEqual(['10002', '10001', '10003']);
    });

    it('reorders bots via store properly (direct swap)', () => {
        botSortStore.reorder('10003', '10001', mockSnapshots);
        expect(botSortStore.get()).toEqual(['10003', '10002', '10001']);
    });
});

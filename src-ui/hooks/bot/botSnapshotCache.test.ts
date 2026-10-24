import { describe, expect, it } from 'vitest';
import type { BotActorSnapshot } from '../../core/ipc/types';
import { mergeBotSnapshotList, updateBotSnapshots } from './botSnapshotCache';

const snapshot = (state: BotActorSnapshot['state'], revision: number): BotActorSnapshot => ({
    bot_id: 'one',
    state,
    revision,
    token_generation: 0,
    pending_restart: false,
});

describe('Bot snapshot ordering', () => {
    it('ignores a late stopping response after a newer running event', () => {
        const running = snapshot('running', 8);
        expect(updateBotSnapshots([running], snapshot('stopping', 7))).toEqual([running]);
    });

    it('keeps events that arrived while the snapshot list was being read', () => {
        const stopped = snapshot('stopped', 9);
        expect(mergeBotSnapshotList([stopped], [snapshot('stopping', 8)])).toEqual([stopped]);
    });

    it('uses list membership to remove deleted Bots and keeps current revisions', () => {
        const removed = { ...snapshot('running', 20), bot_id: 'removed' };
        expect(
            mergeBotSnapshotList([removed, snapshot('running', 3)], [snapshot('stopped', 4)]),
        ).toEqual([snapshot('stopped', 4)]);
    });

    it('accepts a newly registered Bot before the first list has loaded', () => {
        expect(updateBotSnapshots(undefined, snapshot('starting', 1))).toEqual([
            snapshot('starting', 1),
        ]);
    });
});

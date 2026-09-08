import { describe, expect, it } from 'vitest';
import type { BotActorSnapshot } from '../../ipc/types';
import { listActionableBots } from './glance';

function snap(partial: Partial<BotActorSnapshot> & Pick<BotActorSnapshot, 'bot_id' | 'state'>): BotActorSnapshot {
    return {
        revision: 0,
        token_generation: 0,
        pending_restart: false,
        ...partial,
    };
}

describe('listActionableBots', () => {
    it('uses short labels, not last_error text', () => {
        const items = listActionableBots([
            snap({
                bot_id: '10001',
                state: 'crashed',
                last_error: 'RuntimeError: driver does not support http client\nTraceback...',
            }),
            snap({
                bot_id: '10002',
                state: 'running',
                last_error: 'ssh: handshake failed: EOF',
            }),
            snap({
                bot_id: '10003',
                state: 'stopped',
                pending_restart: true,
            }),
        ]);
        expect(items.map((i) => i.detail)).toEqual(['进程异常退出', '运行异常', '等待重启']);
        expect(items.every((i) => !i.detail.includes('Traceback') && !i.detail.includes('ssh:'))).toBe(true);
    });
});

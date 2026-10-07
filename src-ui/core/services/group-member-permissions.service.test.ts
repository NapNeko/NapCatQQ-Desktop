import { describe, expect, it, vi } from 'vitest';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { ProfileMember } from './chat-profile.service';
import { GroupMemberPermissions } from './group-member-permissions.service';

const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};
const member = (id: string, role = id === '99' ? 'owner' : 'member'): ProfileMember => ({
    key: `private:${id}`,
    type: 'private',
    id,
    name: id,
    nickname: '',
    title: '',
    role,
});

describe('member permission prefetch', () => {
    it('shares self reads and caps concurrent prefetch, while ordinary users do not fetch target profiles', async () => {
        let active = 0;
        let peak = 0;
        const read = vi.fn(async (_target: DebugTarget, _group: string, id: string) => {
            active++;
            peak = Math.max(peak, active);
            await Promise.resolve();
            active--;
            return member(id);
        });
        const cache = new GroupMemberPermissions(read);
        await Promise.all(['20', '21', '20', '22', '23'].map((id) => cache.warm(target, '12', id)));
        expect(read.mock.calls.filter((args) => args[2] === '99')).toHaveLength(1);
        expect(read.mock.calls.filter((args) => args[2] === '20')).toHaveLength(1);
        expect(peak).toBeLessThanOrEqual(2);
        cache.clear();
        read.mockClear();
        read.mockImplementation(async (_target, _group, id) => member(id, 'member'));
        await Promise.all(['20', '21', '22'].map((id) => cache.warm(target, '12', id)));
        expect(read.mock.calls.map((args) => args[2])).toEqual(['99']);
    });
    it('shows the existing snapshot after TTL while refreshing, then hides permission after refresh failure', async () => {
        let now = 0;
        const read = vi.fn(async (_target: DebugTarget, _group: string, id: string) => member(id));
        const cache = new GroupMemberPermissions(read, () => now);
        await cache.warm(target, '12', '20');
        now = 30_000;
        let fail!: (reason: Error) => void;
        read.mockImplementation(
            () =>
                new Promise<ProfileMember>((_resolve, reject) => {
                    fail = reject;
                }),
        );
        const refresh = cache.warm(target, '12', '20');
        expect(cache.peek(target, '12', '99')).toMatchObject({
            status: 'loading',
            member: { role: 'owner' },
        });
        expect(cache.peek(target, '12', '20')?.member).toMatchObject({ role: 'member' });
        const shared = cache.warm(target, '12', '20');
        expect(read).toHaveBeenCalledTimes(3);
        fail(new Error('读取失败'));
        await Promise.all([refresh, shared]);
        expect(cache.peek(target, '12', '99')).toMatchObject({ status: 'failed' });
        expect(cache.peek(target, '12', '99')?.member).toBeUndefined();
        now = 301_000;
        expect(cache.peek(target, '12', '20')).toBeUndefined();
    });
    it('clears old scopes and ignores in-flight results after account or group switches', async () => {
        let finish!: (value: ProfileMember) => void;
        const read = vi.fn(async (selected: DebugTarget, _group: string, id: string) =>
            selected.bot_id === 'bot'
                ? new Promise<ProfileMember>((resolve) => {
                      finish = resolve;
                  })
                : member(id),
        );
        const cache = new GroupMemberPermissions(read);
        const old = cache.warm(target, '12', '20');
        const other = { ...target, bot_id: 'another' };
        await cache.self(other, '13');
        finish(member('99'));
        await old;
        expect(cache.peek(target, '12', '99')).toBeUndefined();
        expect(cache.peek(other, '13', '99')?.member?.role).toBe('owner');
        expect(read.mock.calls.some((args) => args[0].bot_id === 'bot' && args[2] === '20')).toBe(
            false,
        );
        cache.clear(other, '13');
        expect(cache.peek(other, '13', '99')).toBeUndefined();
    });
});

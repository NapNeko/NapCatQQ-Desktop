import { describe, expect, it, vi } from 'vitest';
import { chatService } from './chat.service';
import { chatProfileService } from './chat-profile.service';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { Contact } from '../domain/chat/model';
const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};
const friend: Contact = { key: 'private:123', id: '123', type: 'private', name: '已有备注' };
const ok = (data: unknown): DebugCallResponse => ({
    request_id: 'req',
    result: {
        kind: 'ok',
        outcome: {
            ok: true,
            data,
            status: 'ok',
            retcode: 0,
            message: '',
            wording: '',
            raw: {},
            channel: { kind: 'internal' },
            elapsed_ms: 1,
            size_bytes: 0,
            truncated: false,
        },
    },
});
describe('chat profile protocol', () => {
    it('normalizes profile fields without inventing absent personal information', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({
                user_id: 123,
                nickname: '昵称',
                remark: '备注',
                sex: 'unknown',
                age: 0,
                qqLevel: 42,
                long_nick: '个性签名',
            }),
        );
        const profile = await chatProfileService.info(target, friend);
        expect(profile.name).toBe('备注');
        expect(profile.fields).toEqual([
            { label: '昵称', value: '昵称' },
            { label: '等级', value: '42' },
            { label: '签名', value: '个性签名' },
        ]);
    });
    it('uses numeric peer IDs for SnowLuma and rejects unsafe IDs before a request', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ user_id: 123 }));
        await chatProfileService.info({ ...target, backend: 'snowluma' }, friend);
        expect(call).toHaveBeenCalledWith('bot', 'get_stranger_info', {
            user_id: 123,
            no_cache: false,
        });
        await expect(
            chatProfileService.info(
                { ...target, backend: 'snowluma' },
                { ...friend, id: '9007199254740993' },
            ),
        ).rejects.toThrow('无效');
        expect(call).toHaveBeenCalledTimes(1);
    });
    it('sorts owners and admins first and retains searchable nicknames', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok([
                { user_id: 2, nickname: '昵称', card: '群名片', role: 'member' },
                { user_id: 1, nickname: '群主', role: 'owner' },
                { nickname: '坏数据' },
                { user_id: 3, nickname: '管理员', role: 'admin' },
            ]),
        );
        const members = await chatProfileService.members(target, '200');
        expect(members.map((member) => member.id)).toEqual(['1', '3', '2']);
        expect(members[2]).toMatchObject({
            name: '群名片',
            nickname: '昵称',
            key: 'private:2',
            type: 'private',
        });
    });
    it('rejects truncated or malformed member responses instead of showing an empty group', async () => {
        const response = ok([]);
        if (response.result.kind === 'ok') response.result.outcome.truncated = true;
        vi.spyOn(chatService, 'call')
            .mockResolvedValueOnce(response)
            .mockResolvedValueOnce(ok({ unexpected: true }));
        await expect(chatProfileService.members(target, '200')).rejects.toThrow('内容过大');
        await expect(chatProfileService.members(target, '200')).rejects.toThrow('成员列表格式');
    });
    it('reads a fresh member card and rejects a response for a different group', async () => {
        const call = vi
            .spyOn(chatService, 'call')
            .mockResolvedValueOnce(
                ok({
                    group_id: 200,
                    user_id: 123,
                    card: '群名片',
                    role: 'member',
                    shut_up_timestamp: 123456,
                }),
            )
            .mockResolvedValueOnce(ok({ group_id: 300, user_id: 123, role: 'member' }));
        const member = await chatProfileService.member(
            { ...target, backend: 'snowluma' },
            '200',
            '123',
        );
        expect(call).toHaveBeenCalledWith('bot', 'get_group_member_info', {
            group_id: 200,
            user_id: 123,
            no_cache: true,
        });
        expect(member).toMatchObject({
            id: '123',
            name: '群名片',
            role: 'member',
            mutedUntil: 123456,
        });
        await expect(chatProfileService.member(target, '200', '123')).rejects.toThrow(
            '不属于当前群',
        );
    });
    it('sends explicit mute duration and allows a removed member to reapply', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok(null));
        await chatProfileService.ban(target, '200', '123', 0);
        await chatProfileService.kick(target, '200', '123');
        expect(call).toHaveBeenNthCalledWith(1, 'bot', 'set_group_ban', {
            group_id: '200',
            user_id: '123',
            duration: 0,
        });
        expect(call).toHaveBeenNthCalledWith(2, 'bot', 'set_group_kick', {
            group_id: '200',
            user_id: '123',
            reject_add_request: false,
        });
        await expect(chatProfileService.ban(target, '200', '123', 31 * 86400)).rejects.toThrow(
            '30 天',
        );
        expect(call).toHaveBeenCalledTimes(2);
    });
});

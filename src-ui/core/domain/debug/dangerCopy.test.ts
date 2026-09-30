import { describe, expect, it } from 'vitest';
import { dangerConsequence } from './dangerCopy';

const FALLBACK = '这个接口会对 QQ 产生不可撤销的影响';

describe('dangerConsequence', () => {
    it('bot_exit / set_restart 说明是哪个 Bot', () => {
        expect(dangerConsequence('bot_exit', {}, '小雪')).toBe('小雪 会退出登录，需要重新登录');
        expect(dangerConsequence('set_restart', {}, '小雪')).toBe('小雪 会重启');
    });

    it('踢人带上被踢的人和群（字符串 id 和数字 id 都行）', () => {
        expect(dangerConsequence('set_group_kick', { group_id: 100001, user_id: 10001 }, 'x')).toBe('会把 10001 移出群 100001');
        expect(dangerConsequence('set_group_kick', { group_id: '100001', user_id: '10001' }, 'x')).toBe('会把 10001 移出群 100001');
    });

    it('参数没填时看得出来，而不是写成 undefined', () => {
        expect(dangerConsequence('set_group_kick', {}, 'x')).toBe('会把 （未填 user_id） 移出群 （未填 group_id）');
    });

    it('号类参数是 0 也当没填：不会出现「会撤回消息 0」', () => {
        expect(dangerConsequence('delete_msg', { message_id: 0 }, 'x')).toBe('会撤回消息 （未填 message_id）');
        expect(dangerConsequence('delete_msg', { message_id: '0' }, 'x')).toBe('会撤回消息 （未填 message_id）');
        expect(dangerConsequence('set_group_kick', { group_id: 0, user_id: 0 }, 'x')).toBe('会把 （未填 user_id） 移出群 （未填 group_id）');
        // 不是号的参数，0 就是 0
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: 0 }, 'x')).toBe('会解除 2 在群 1 的禁言');
    });

    it('批量踢人数人头', () => {
        expect(dangerConsequence('set_group_kick_members', { group_id: 1, user_id: [2, 3, 4] }, 'x')).toBe('会把 3 个人移出群 1');
        expect(dangerConsequence('set_group_kick_members', { group_id: 1 }, 'x')).toBe('会批量移出群 1 的成员');
    });

    it('禁言按秒说，明确写了 0 才是解除禁言', () => {
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: 600 }, 'x')).toBe('会禁言 2 600 秒');
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: '1800' }, 'x')).toBe('会禁言 2 1800 秒');
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: 0 }, 'x')).toBe('会解除 2 在群 1 的禁言');
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: '0' }, 'x')).toBe('会解除 2 在群 1 的禁言');
    });

    it('禁言没填 duration：不能说成解除，写明上游默认 1800 秒', () => {
        const unset = '会禁言 2 （未填 duration，上游默认 1800 秒）';
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2 }, 'x')).toBe(unset);
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: '' }, 'x')).toBe(unset);
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: '  ' }, 'x')).toBe(unset);
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: null }, 'x')).toBe(unset);
        expect(dangerConsequence('set_group_ban', { group_id: 1, user_id: 2, duration: undefined }, 'x')).not.toContain('解除');
    });

    it('全员禁言看 enable，缺省是开启，字符串 "false" 也识别', () => {
        expect(dangerConsequence('set_group_whole_ban', { group_id: 1 }, 'x')).toBe('会对群 1 开启全员禁言');
        expect(dangerConsequence('set_group_whole_ban', { group_id: 1, enable: false }, 'x')).toBe('会关闭群 1 的全员禁言');
        expect(dangerConsequence('set_group_whole_ban', { group_id: 1, enable: 'false' }, 'x')).toBe('会关闭群 1 的全员禁言');
    });

    it('退群 / 解散、设管理员', () => {
        expect(dangerConsequence('set_group_leave', { group_id: 1 }, 'x')).toBe('会退出群 1');
        expect(dangerConsequence('set_group_leave', { group_id: 1, is_dismiss: true }, 'x')).toBe('会解散群 1（仅群主可以）');
        expect(dangerConsequence('set_group_admin', { group_id: 1, user_id: 2 }, 'x')).toBe('会把 2 设为群 1 的管理员');
        expect(dangerConsequence('set_group_admin', { group_id: 1, user_id: 2, enable: false }, 'x')).toBe('会取消 2 在群 1 的管理员');
    });

    it('删除类', () => {
        expect(dangerConsequence('delete_msg', { message_id: 77 }, 'x')).toBe('会撤回消息 77');
        expect(dangerConsequence('delete_friend', { user_id: 2 }, 'x')).toBe('会删除好友 2');
        expect(dangerConsequence('clean_cache', {}, '小雪')).toBe('会清空 小雪 的缓存文件');
        expect(dangerConsequence('delete_group_file', { group_id: 1, file_id: 'f1' }, 'x')).toBe('会删除群 1 里的文件 f1');
        expect(dangerConsequence('delete_group_folder', { group_id: 1, folder_id: 'd1' }, 'x')).toBe('会删除群 1 里的文件夹 d1');
        expect(dangerConsequence('_del_group_notice', { group_id: 1 }, 'x')).toBe('会删除群 1 的一条公告');
        expect(dangerConsequence('delete_flash_file', {}, 'x')).toBe('会删除这个闪传文件');
    });

    it('没有专门文案的走兜底句', () => {
        expect(dangerConsequence('some_future_dangerous_action', {}, 'x')).toBe(FALLBACK);
    });
});

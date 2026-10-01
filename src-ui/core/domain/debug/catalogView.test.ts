import { describe, expect, it } from 'vitest';
import type { DebugActionCategory } from '../../ipc/generated/debug/DebugActionCategory';
import type { DebugActionSummary } from '../../ipc/generated/debug/DebugActionSummary';
import { CATEGORY_LABEL, CATEGORY_ORDER, catalogRowLabel, groupActions, searchActions } from './catalogView';

const action = (name: string, over: Partial<DebugActionSummary> = {}): DebugActionSummary => ({
    name,
    aliases: [],
    summary: '',
    category: 'message',
    safety: 'read_only',
    stream: false,
    supported: true,
    other_backend_present: null,
    param_diff: false,
    ...over,
});

const names = (list: DebugActionSummary[]) => list.map((a) => a.name);

describe('CATEGORY_LABEL / CATEGORY_ORDER', () => {
    it('每个分类都有中文名', () => {
        expect(CATEGORY_LABEL).toEqual({
            message: '消息',
            group_info: '群信息',
            group_admin: '群管理',
            friend: '好友',
            file: '文件',
            request: '请求处理',
            account: '账号与状态',
            face: '表情',
            stream: '流式',
            extension: '扩展',
        });
    });

    it('顺序覆盖全部分类且不重复', () => {
        expect(CATEGORY_ORDER.map((c) => CATEGORY_LABEL[c])).toEqual([
            '消息',
            '群信息',
            '群管理',
            '好友',
            '文件',
            '请求处理',
            '账号与状态',
            '表情',
            '流式',
            '扩展',
        ]);
        expect(new Set(CATEGORY_ORDER).size).toBe(Object.keys(CATEGORY_LABEL).length);
    });
});

describe('groupActions', () => {
    it('按分类顺序分组，空分类不出现，组内按名字排', () => {
        const list = [
            action('set_group_kick', { category: 'group_admin' }),
            action('send_msg', { category: 'message' }),
            action('get_login_info', { category: 'account' }),
            action('delete_msg', { category: 'message' }),
            action('get_group_list', { category: 'group_info' }),
        ];
        const { groups, unsupported } = groupActions(list);
        expect(groups.map((g) => [g.category, g.label, names(g.actions)])).toEqual([
            ['message', '消息', ['delete_msg', 'send_msg']],
            ['group_info', '群信息', ['get_group_list']],
            ['group_admin', '群管理', ['set_group_kick']],
            ['account', '账号与状态', ['get_login_info']],
        ]);
        expect(unsupported).toEqual([]);
    });

    it('当前 Bot 不支持的单独收起来，不占分类', () => {
        const list = [
            action('nc_only', { category: 'extension', supported: false }),
            action('send_msg'),
            action('a_unsupported', { category: 'message', supported: false }),
        ];
        const { groups, unsupported } = groupActions(list);
        expect(groups.map((g) => g.category)).toEqual(['message']);
        expect(names(groups[0]!.actions)).toEqual(['send_msg']);
        expect(names(unsupported)).toEqual(['a_unsupported', 'nc_only']);
    });

    it('下划线开头的内部接口排在组末尾', () => {
        const list = [action('_del_group_notice'), action('zzz_last'), action('aaa_first')];
        expect(names(groupActions(list).groups[0]!.actions)).toEqual(['aaa_first', 'zzz_last', '_del_group_notice']);
    });

    it('空列表', () => {
        expect(groupActions([])).toEqual({ groups: [], unsupported: [] });
    });

    it('不改传入的数组', () => {
        const list = [action('b'), action('a')];
        groupActions(list);
        expect(names(list)).toEqual(['b', 'a']);
    });
});

describe('searchActions', () => {
    const list = [
        action('send_group_msg', { summary: '发送群消息', aliases: ['send_group_message'] }),
        action('send_msg', { summary: '发送消息' }),
        action('get_msg', { summary: '获取消息' }),
        action('delete_msg', { summary: '撤回消息' }),
        action('send', { summary: '通用发送' }),
        action('get_group_msg_history', { summary: '获取群历史消息', aliases: ['get_history'] }),
        action('set_qq_profile', { summary: '设置资料，可用于 send 测试' }),
    ];

    it('打分：名字完全一致 > 前缀 > 别名完全一致 > 名字包含 > 简介包含', () => {
        // send：完全一致 100；send_group_msg / send_msg 前缀 80；其余里 set_qq_profile 只有简介含 send
        expect(names(searchActions(list, 'send', []))).toEqual(['send', 'send_group_msg', 'send_msg', 'set_qq_profile']);

        const alias = [
            action('zzz_action', { aliases: ['hist'] }),
            action('my_hist_thing'),
            action('other', { summary: '含 hist 的简介' }),
        ];
        expect(names(searchActions(alias, 'hist', []))).toEqual(['zzz_action', 'my_hist_thing', 'other']);
    });

    it('大小写不敏感，前后空白忽略', () => {
        expect(names(searchActions(list, '  GET_MSG ', []))).toEqual(['get_msg']);
    });

    it('能搜中文简介', () => {
        expect(names(searchActions(list, '撤回', []))).toEqual(['delete_msg']);
        // 都只是简介包含，同分按名字
        expect(names(searchActions(list, '消息', []))).toEqual([
            'delete_msg',
            'get_group_msg_history',
            'get_msg',
            'send_group_msg',
            'send_msg',
        ]);
    });

    it('没有命中返回空', () => {
        expect(searchActions(list, 'zzzz_nothing', [])).toEqual([]);
    });

    it('最近用过的加 10 分，能压过原本略高一档的', () => {
        // 'msg'：get_msg / send_msg / delete_msg / send_group_msg 都是「名字包含」60；
        // get_group_msg_history 也含 msg。全是 60，最近用过的靠前
        const out = names(searchActions(list, 'msg', ['delete_msg']));
        expect(out[0]).toBe('delete_msg');
        // 前缀 80 与「包含 60 + 最近 10 = 70」比，前缀仍然更高
        const prefix = [action('msg_tool'), action('a_msg')];
        expect(names(searchActions(prefix, 'msg', ['a_msg']))).toEqual(['msg_tool', 'a_msg']);
    });

    it('同分时最近用的靠前（新的在前），再同分按名字', () => {
        const same = [action('x_msg_a'), action('x_msg_b'), action('x_msg_c'), action('x_msg_d')];
        expect(names(searchActions(same, 'msg', ['x_msg_c', 'x_msg_b']))).toEqual(['x_msg_c', 'x_msg_b', 'x_msg_a', 'x_msg_d']);
    });

    it('空查询：最近用过的在前（最近的最前），其余按名字', () => {
        const items = [action('c'), action('a'), action('b'), action('d')];
        expect(names(searchActions(items, '', ['d', 'b']))).toEqual(['d', 'b', 'a', 'c']);
        expect(names(searchActions(items, '   ', []))).toEqual(['a', 'b', 'c', 'd']);
    });

    it('recent 里有目录外的名字、重复的名字也不出错', () => {
        const items = [action('a'), action('b')];
        expect(names(searchActions(items, '', ['ghost', 'b', 'b']))).toEqual(['b', 'a']);
    });

    it('不支持的接口也参与搜索（由界面决定怎么显示）', () => {
        const items = [action('nc_only', { supported: false })];
        expect(names(searchActions(items, 'nc_only', []))).toEqual(['nc_only']);
    });

    it('分类无关', () => {
        const cats: DebugActionCategory[] = ['message', 'friend'];
        const items = cats.map((c, i) => action(`act_${i}`, { category: c }));
        expect(searchActions(items, 'act', [])).toHaveLength(2);
    });
});

describe('catalogRowLabel', () => {
    it('剥掉中文括注', () => {
        expect(catalogRowLabel(action('get_forward_msg', { summary: '获取合并转发消息（id 或 message_id）' }))).toBe('获取合并转发消息');
        expect(catalogRowLabel(action('ocr_image', { summary: 'OCR 图片（服务端，需图片 URL 或已缓存的图片 file_id）' }))).toBe('OCR 图片');
    });

    it('剥掉半角括注', () => {
        expect(catalogRowLabel(action('mark_msg_as_read', { summary: '标记消息已读 (Go-CQHTTP)' }))).toBe('标记消息已读');
    });

    it('剥掉分号后的补充', () => {
        expect(catalogRowLabel(action('get_record', { summary: '获取语音信息；传 out_format 则服务端转码并附带 base64' }))).toBe('获取语音信息');
    });

    it('普通简介原样返回', () => {
        expect(catalogRowLabel(action('get_group_list', { summary: '获取群列表' }))).toBe('获取群列表');
        expect(catalogRowLabel(action('send_ark_share', { summary: '分享用户/群 Ark 卡片' }))).toBe('分享用户/群 Ark 卡片');
    });

    it('没有简介、或整个简介就是一条注释时退回字段名', () => {
        expect(catalogRowLabel(action('get_msg'))).toBe('get_msg');
        expect(catalogRowLabel(action('_get_model_show', { summary: '（占位）' }))).toBe('_get_model_show');
    });
});

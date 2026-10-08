// 好友与请求处理类动作的定义。

import { type ActionDef, arr, noCache, obj, T, uid, COMMON_ERRORS } from './catalog-schema';

export const DEFS_FRIEND: ActionDef[] = [
    // —— 好友
    {
        name: 'get_friend_list',
        summary: '获取好友列表',
        category: 'friend',
        safety: 'read_only',
        params: [noCache()],
        returns: arr(obj({ user_id: T.int, nickname: T.str, remark: T.str })),
        returnData: [{ user_id: 10001, nickname: '小明', remark: '' }],
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'get_stranger_info',
        summary: '获取陌生人资料',
        category: 'friend',
        safety: 'read_only',
        params: [uid(), noCache()],
        returns: obj({
            user_id: T.int,
            nickname: T.str,
            sex: T.str,
            age: T.int,
            qid: T.str,
            long_nick: T.str,
        }),
        example: { user_id: '10001' },
        returnData: {
            user_id: 10001,
            nickname: '小明',
            sex: 'unknown',
            age: 0,
            qid: '',
            long_nick: '',
        },
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'send_like',
        summary: '给好友资料卡点赞',
        category: 'friend',
        safety: 'side_effect',
        params: [
            uid(),
            {
                name: 'times',
                type: 'integer',
                desc: '点赞次数，每人每天上限 20',
                default: 1,
                minimum: 1,
                maximum: 20,
                requiredOn: { snowluma: true },
            },
        ],
        returns: T.nul,
        example: { user_id: '10001', times: 3 },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '今日点赞次数已达上限' }, ...COMMON_ERRORS],
    },
];

export const DEFS_REQUEST: ActionDef[] = [
    // —— 请求处理
    {
        name: 'set_friend_add_request',
        summary: '处理加好友请求',
        category: 'request',
        safety: 'side_effect',
        params: [
            { name: 'flag', type: 'string', desc: '请求 flag，取自加好友请求事件', required: true },
            { name: 'approve', type: 'boolean', desc: '是否同意', default: true },
            { name: 'remark', type: 'string', desc: '同意后给对方的备注' },
        ],
        returns: T.nul,
        example: { flag: '10005_1759190400_0', approve: true },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '请求已过期' }, ...COMMON_ERRORS],
    },
    {
        name: 'set_group_add_request',
        summary: '处理加群请求 / 邀请',
        category: 'request',
        safety: 'side_effect',
        params: [
            { name: 'flag', type: 'string', desc: '请求 flag，取自加群请求事件', required: true },
            { name: 'sub_type', desc: '请求类型', values: ['add', 'invite'], required: true },
            { name: 'approve', type: 'boolean', desc: '是否同意', default: true },
            { name: 'reason', type: 'string', desc: '拒绝理由' },
        ],
        returns: T.nul,
        example: { flag: '20001', sub_type: 'add', approve: false, reason: '不认识' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '请求已过期' }, ...COMMON_ERRORS],
    },
];

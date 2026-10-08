// 消息收发类动作的定义。

import {
    type ActionDef,
    arr,
    autoEscape,
    COMMON_ERRORS,
    gid,
    MESSAGE_ID_ROW,
    message,
    obj,
    SEND_ERRORS,
    T,
    uid,
} from './catalog-schema';

export const DEFS_MESSAGE: ActionDef[] = [
    // —— 消息
    {
        name: 'send_group_msg',
        summary: '发送群消息',
        description:
            '向指定群发送消息。`message` 可以是消息段数组，也可以是带 CQ 码的字符串；返回新消息的 message_id。',
        category: 'message',
        safety: 'side_effect',
        params: [gid(), message(), autoEscape()],
        returns: MESSAGE_ID_ROW,
        example: { group_id: '100001', message: [{ type: 'text', data: { text: '你好' } }] },
        returnData: { message_id: 1700000001 },
        errorExamples: SEND_ERRORS,
        invariants: ['message 不能为空', '群号必须是 Bot 已加入的群'],
    },
    {
        name: 'send_private_msg',
        summary: '发送私聊消息',
        category: 'message',
        safety: 'side_effect',
        params: [
            uid(),
            { name: 'group_id', role: 'group_id', desc: '临时会话来源群，好友不用填' },
            message(),
            autoEscape(),
        ],
        returns: MESSAGE_ID_ROW,
        example: { user_id: '10001', message: 'hello' },
        returnData: { message_id: 1700000002 },
        errorExamples: SEND_ERRORS,
        invariants: ['message 不能为空'],
    },
    {
        name: 'send_msg',
        summary: '发送消息（自动判断群聊或私聊）',
        category: 'message',
        safety: 'side_effect',
        params: [
            {
                name: 'message_type',
                desc: '消息类型，不填则按 group_id / user_id 判断',
                values: ['group', 'private'],
            },
            { name: 'user_id', role: 'user_id', desc: '对方 QQ 号，私聊时必填' },
            { name: 'group_id', role: 'group_id', desc: '群号，群聊时必填' },
            message(),
            autoEscape(),
        ],
        returns: MESSAGE_ID_ROW,
        example: { message_type: 'group', group_id: '100001', message: 'hello' },
        returnData: { message_id: 1700000003 },
        errorExamples: SEND_ERRORS,
    },
    {
        name: 'get_msg',
        summary: '获取消息详情',
        category: 'message',
        safety: 'read_only',
        params: [{ name: 'message_id', role: 'message_id', desc: '消息 ID', required: true }],
        returns: obj({
            time: T.int,
            message_type: T.str,
            message_id: T.int,
            real_id: T.int,
            sender: obj({ user_id: T.int, nickname: T.str }),
            message: arr({ type: 'object' }),
        }),
        example: { message_id: '1700000001' },
        returnData: {
            time: 1759190400,
            message_type: 'group',
            message_id: 1700000001,
            real_id: 1,
            sender: { user_id: 10001, nickname: '小明' },
            message: [{ type: 'text', data: { text: '你好' } }],
        },
        errorExamples: [{ retcode: 1200, message: '消息不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'delete_msg',
        aliases: ['recall_msg'],
        summary: '撤回消息',
        description:
            '撤回一条消息。Bot 只能撤回自己 2 分钟内发的消息，或在有管理权限时撤回别人的。',
        category: 'message',
        safety: 'dangerous',
        params: [
            { name: 'message_id', role: 'message_id', desc: '要撤回的消息 ID', required: true },
        ],
        returns: T.nul,
        example: { message_id: '1700000001' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '消息不存在或已超过撤回时限' }, ...COMMON_ERRORS],
        invariants: ['已撤回的消息再撤回会失败'],
    },
    {
        name: 'get_forward_msg',
        summary: '获取合并转发内容',
        category: 'message',
        safety: 'read_only',
        params: [
            {
                name: 'message_id',
                role: 'message_id',
                desc: '合并转发的消息 ID 或 res_id',
                required: true,
            },
        ],
        returns: obj({ messages: arr({ type: 'object' }) }),
        returnsText: '对象：messages，每项含 sender、time、content',
        example: { message_id: '7300000000000000001' },
        returnData: {
            messages: [
                {
                    sender: { user_id: 10001, nickname: '小明' },
                    time: 1759190400,
                    message: [{ type: 'text', data: { text: '第一条' } }],
                },
            ],
        },
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'send_group_forward_msg',
        summary: '发送群合并转发',
        category: 'message',
        safety: 'side_effect',
        params: [
            gid(),
            {
                name: 'messages',
                type: 'array',
                desc: '转发节点列表，每个节点是 node 消息段',
                required: true,
            },
        ],
        returns: obj({ message_id: T.int, res_id: T.str }),
        example: {
            group_id: '100001',
            messages: [
                {
                    type: 'node',
                    data: {
                        user_id: '10001',
                        nickname: '小明',
                        content: [{ type: 'text', data: { text: '早' } }],
                    },
                },
            ],
        },
        returnData: { message_id: 1700000004, res_id: 'res_0001' },
        errorExamples: SEND_ERRORS,
    },
];

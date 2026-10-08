// 账号与状态类动作的定义。

import { type ActionDef, obj, T, COMMON_ERRORS } from './catalog-schema';

export const DEFS_ACCOUNT: ActionDef[] = [
    // —— 账号与状态
    {
        name: 'get_login_info',
        aliases: ['get_self_info'],
        summary: '获取登录号信息',
        description:
            '返回当前登录的 QQ 号和昵称。调试台里用它确认 Bot 是不是在线、连的是不是想连的那个号。',
        category: 'account',
        safety: 'read_only',
        params: [],
        returns: obj({ user_id: T.int, nickname: T.str }),
        returnData: { user_id: 1919810, nickname: 'NapCat 测试号' },
        errorExamples: COMMON_ERRORS,
        invariants: ['未登录时返回失败'],
    },
    {
        name: 'get_status',
        summary: '获取运行状态',
        category: 'account',
        safety: 'read_only',
        params: [],
        returns: obj({ online: T.bool, good: T.bool, stat: obj({}) }),
        returnData: { online: true, good: true, stat: {} },
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'get_version_info',
        aliases: ['get_version'],
        summary: '获取版本信息',
        category: 'account',
        safety: 'read_only',
        params: [],
        returns: obj({ app_name: T.str, app_version: T.str, protocol_version: T.str }),
        returnsText: '对象：app_name、app_version、protocol_version',
        returnData: { app_name: 'NapCat.Onebot', app_version: '4.15.18', protocol_version: 'v11' },
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'bot_exit',
        summary: '让 Bot 退出登录并结束进程',
        description:
            '会让当前 QQ 下线，进程退出后需要在 Bot 页重新启动。仅在确实要关掉这个号时使用。',
        category: 'account',
        safety: 'dangerous',
        params: [],
        returns: T.nul,
        returnData: null,
        errorExamples: COMMON_ERRORS,
        invariants: ['调用成功后连接会被上游主动断开'],
    },
];

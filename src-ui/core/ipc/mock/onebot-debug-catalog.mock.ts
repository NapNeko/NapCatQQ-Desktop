// 调试台浏览器预览用的接口目录：手写 32 个动作（每个后端 31 个），覆盖全部分类和三档安全等级。
// 其中 `debug_huge_response` 是预览专用的假动作，用来走查超大回包被截断的界面。
//
// 每个动作只写一份定义，再按后端生成完整的 `DebugActionSpec`：
//   NapCat  —— id 类参数是字符串，布尔参数是 `anyOf[boolean, string]`，带返回结构、示例和报错样例（真实快照的样子）
//   SnowLuma —— id 类参数是整数，布尔就是 boolean，没有示例，返回值常常只有一段文字，带 invariants
// 两边的差异（参数只在一边有、必填不同、类型不同）由生成结果直接比出来，和后端转换器的做法一致。

import type { BackendType } from '../generated/domain/BackendType';
import type { DebugActionCategory } from '../generated/debug/DebugActionCategory';
import type { DebugActionSafety } from '../generated/debug/DebugActionSafety';
import type { DebugActionSpec } from '../generated/debug/DebugActionSpec';
import type { DebugActionSummary } from '../generated/debug/DebugActionSummary';
import type { DebugCatalog } from '../generated/debug/DebugCatalog';
import type { DebugCatalogSource } from '../generated/debug/DebugCatalogSource';
import type { DebugErrorExample } from '../generated/debug/DebugErrorExample';
import type { DebugParamDiff } from '../generated/debug/DebugParamDiff';

type Role =
    | 'group_id'
    | 'user_id'
    | 'member_id'
    | 'message_id'
    | 'message'
    | 'file'
    | 'image'
    | 'record'
    | 'video'
    | 'face_id'
    | 'timestamp';

const ID_ROLES: ReadonlySet<string> = new Set(['group_id', 'user_id', 'member_id', 'message_id']);

interface ParamDef {
    name: string;
    desc: string;
    role?: Role;
    type?: 'string' | 'integer' | 'number' | 'boolean' | 'array' | 'object';
    values?: Array<string | number>;
    required?: boolean;
    default?: unknown;
    minimum?: number;
    maximum?: number;
    /** 只有这个后端有这个参数 */
    only?: BackendType;
    /** 两个后端对必填的看法不同时，按后端覆盖 */
    requiredOn?: Partial<Record<BackendType, boolean>>;
    /** 两个后端声明的类型不同时，按后端覆盖 */
    typeOn?: Partial<Record<BackendType, 'string' | 'integer' | 'number'>>;
}

interface ActionDef {
    name: string;
    aliases?: string[];
    summary: string;
    /** NapCat 快照里的长说明；SnowLuma 没有 */
    description?: string;
    category: DebugActionCategory;
    safety: DebugActionSafety;
    stream?: boolean;
    /** 哪些后端有这个动作，不写就是两边都有 */
    backends?: BackendType[];
    params: ParamDef[];
    returns?: Record<string, unknown>;
    /** SnowLuma 的 `returns` 是散文；写了它，SnowLuma 就不给返回结构 */
    returnsText?: string;
    /** NapCat 的请求示例，id 按 NapCat 的习惯写成字符串 */
    example?: Record<string, unknown>;
    returnData?: unknown;
    errorExamples?: DebugErrorExample[];
    invariants?: string[];
}

// ---------------------------------------------------------------------------
// 参数和返回结构的小零件
// ---------------------------------------------------------------------------

const gid = (extra: Partial<ParamDef> = {}): ParamDef => ({
    name: 'group_id',
    role: 'group_id',
    desc: '群号',
    required: true,
    ...extra,
});
const uid = (extra: Partial<ParamDef> = {}): ParamDef => ({
    name: 'user_id',
    role: 'user_id',
    desc: 'QQ 号',
    required: true,
    ...extra,
});
const member = (desc = '群成员的 QQ 号'): ParamDef => ({
    name: 'user_id',
    role: 'member_id',
    desc,
    required: true,
});
const message = (): ParamDef => ({
    name: 'message',
    role: 'message',
    desc: '要发送的内容，消息段数组或 CQ 码文本',
    required: true,
});
const noCache = (extra: Partial<ParamDef> = {}): ParamDef => ({
    name: 'no_cache',
    type: 'boolean',
    desc: '不使用缓存',
    default: false,
    ...extra,
});
const autoEscape = (): ParamDef => ({
    name: 'auto_escape',
    type: 'boolean',
    desc: '消息内容当作纯文本发送，不解析 CQ 码',
    default: false,
});

const T = {
    int: { type: 'integer' },
    str: { type: 'string' },
    bool: { type: 'boolean' },
    nul: { type: 'null' },
} as const;
const obj = (properties: Record<string, unknown>) => ({ type: 'object', properties });
const arr = (items: unknown) => ({ type: 'array', items });

const GROUP_ROW = obj({
    group_id: T.int,
    group_name: T.str,
    member_count: T.int,
    max_member_count: T.int,
    group_all_shut: T.int,
    group_remark: T.str,
});
const MEMBER_ROW = obj({
    group_id: T.int,
    user_id: T.int,
    nickname: T.str,
    card: T.str,
    sex: T.str,
    age: T.int,
    join_time: T.int,
    last_sent_time: T.int,
    level: T.str,
    role: { type: 'string', enum: ['owner', 'admin', 'member'] },
    title: T.str,
    shut_up_timestamp: T.int,
});
const MESSAGE_ID_ROW = obj({ message_id: T.int });

/** 样例回包的外壳：NapCat 的示例是完整的 OB11 回复 */
const reply = (data: unknown) => ({
    status: 'ok',
    retcode: 0,
    data,
    message: '',
    wording: '',
    echo: null,
});

const COMMON_ERRORS: DebugErrorExample[] = [
    { retcode: 1400, message: '请求参数错误' },
    { retcode: 1404, message: '不支持的 API' },
];
const SEND_ERRORS: DebugErrorExample[] = [
    { retcode: 1200, message: '消息发送失败：群不存在或未加入' },
    { retcode: 1400, message: '请求参数错误' },
];

// ---------------------------------------------------------------------------
// 动作定义
// ---------------------------------------------------------------------------

const DEFS: ActionDef[] = [
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

    // —— 群信息
    {
        name: 'get_group_list',
        summary: '获取群列表',
        category: 'group_info',
        safety: 'read_only',
        params: [noCache()],
        returns: arr(GROUP_ROW),
        returnData: [
            {
                group_id: 100001,
                group_name: '测试群 1',
                member_count: 500,
                max_member_count: 500,
                group_all_shut: 0,
                group_remark: '',
            },
        ],
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'get_group_info',
        summary: '获取群资料',
        category: 'group_info',
        safety: 'read_only',
        params: [gid(), noCache()],
        returns: GROUP_ROW,
        example: { group_id: '100001' },
        returnData: {
            group_id: 100001,
            group_name: '测试群 1',
            member_count: 500,
            max_member_count: 500,
            group_all_shut: 0,
            group_remark: '',
        },
        errorExamples: [{ retcode: 1200, message: '群不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'get_group_member_list',
        summary: '获取群成员列表',
        description: '大群会比较慢（几百人的群要接近一秒），调试台默认给它更长的等待时间。',
        category: 'group_info',
        safety: 'read_only',
        params: [gid(), noCache({ only: 'napcat' })],
        returns: arr(MEMBER_ROW),
        example: { group_id: '100001' },
        returnData: [
            {
                group_id: 100001,
                user_id: 10001,
                nickname: '小明',
                card: '',
                role: 'owner',
                title: '',
            },
        ],
        errorExamples: [{ retcode: 1200, message: '群不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'get_group_member_info',
        summary: '获取群成员资料',
        category: 'group_info',
        safety: 'read_only',
        params: [gid(), member(), noCache()],
        returns: MEMBER_ROW,
        returnsText: '对象：与群成员列表里的一项相同',
        example: { group_id: '100001', user_id: '10001' },
        returnData: {
            group_id: 100001,
            user_id: 10001,
            nickname: '小明',
            card: '',
            role: 'owner',
            title: '',
        },
        errorExamples: [{ retcode: 1200, message: '群成员不存在' }, ...COMMON_ERRORS],
    },

    // —— 群管理
    {
        name: 'set_group_ban',
        summary: '禁言群成员',
        description: '把成员禁言指定秒数，`duration` 为 0 表示解除禁言。最长 30 天。',
        category: 'group_admin',
        safety: 'dangerous',
        params: [
            gid(),
            member('要禁言的成员'),
            {
                name: 'duration',
                type: 'integer',
                desc: '禁言秒数，0 为解除',
                default: 1800,
                minimum: 0,
                maximum: 2592000,
                typeOn: { napcat: 'number' },
            },
        ],
        returns: T.nul,
        example: { group_id: '100001', user_id: '10002', duration: 60 },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '权限不足' }, ...COMMON_ERRORS],
        invariants: ['Bot 必须是群主或管理员', '不能禁言群主'],
    },
    {
        name: 'set_group_kick',
        summary: '移出群成员',
        category: 'group_admin',
        safety: 'dangerous',
        params: [
            gid(),
            member('要移出的成员'),
            {
                name: 'reject_add_request',
                type: 'boolean',
                desc: '同时拒绝这个人再次加群',
                default: false,
            },
        ],
        returns: T.nul,
        example: { group_id: '100001', user_id: '10002' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '权限不足' }, ...COMMON_ERRORS],
        invariants: ['Bot 必须是群主或管理员'],
    },
    {
        name: 'set_group_whole_ban',
        summary: '开关全员禁言',
        category: 'group_admin',
        safety: 'dangerous',
        params: [
            gid(),
            { name: 'enable', type: 'boolean', desc: 'true 开启，false 关闭', default: true },
        ],
        returns: T.nul,
        example: { group_id: '100001', enable: true },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '权限不足' }, ...COMMON_ERRORS],
        invariants: ['Bot 必须是群主或管理员'],
    },
    {
        name: 'set_group_card',
        summary: '设置群名片',
        category: 'group_admin',
        safety: 'side_effect',
        params: [
            gid(),
            member('要改名片的成员'),
            { name: 'card', type: 'string', desc: '新名片，留空为清除' },
        ],
        returns: T.nul,
        example: { group_id: '100001', user_id: '10002', card: '测试员' },
        returnData: null,
        errorExamples: COMMON_ERRORS,
    },

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

    // —— 文件
    {
        name: 'upload_group_file',
        summary: '上传群文件',
        category: 'file',
        safety: 'side_effect',
        params: [
            gid(),
            {
                name: 'file',
                role: 'file',
                type: 'string',
                desc: 'Bot 所在机器上的文件路径，或 http(s) 地址',
                required: true,
            },
            { name: 'name', type: 'string', desc: '文件在群里显示的名字', required: true },
            {
                name: 'folder_id',
                type: 'string',
                desc: '目标文件夹 ID，不填为根目录',
                only: 'napcat',
            },
            {
                name: 'folder',
                type: 'string',
                desc: '目标文件夹 ID，不填为根目录',
                only: 'snowluma',
            },
        ],
        returns: T.nul,
        example: { group_id: '100001', file: '/tmp/report.pdf', name: 'report.pdf' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '文件不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'get_group_file_url',
        summary: '获取群文件下载链接',
        category: 'file',
        safety: 'read_only',
        params: [
            gid(),
            { name: 'file_id', type: 'string', desc: '文件 ID', required: true },
            { name: 'busid', type: 'integer', desc: '文件类型 ID', default: 102 },
        ],
        returns: obj({ url: T.str }),
        example: { group_id: '100001', file_id: '/abcd-1234' },
        returnData: { url: 'https://example.invalid/file/abcd-1234' },
        errorExamples: [{ retcode: 1200, message: '文件不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'delete_group_file',
        summary: '删除群文件',
        category: 'file',
        safety: 'dangerous',
        params: [
            gid(),
            { name: 'file_id', type: 'string', desc: '文件 ID', required: true },
            { name: 'busid', type: 'integer', desc: '文件类型 ID', default: 102 },
        ],
        returns: T.nul,
        example: { group_id: '100001', file_id: '/abcd-1234' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '权限不足或文件不存在' }, ...COMMON_ERRORS],
        invariants: ['只能删除自己上传的文件，除非有管理权限'],
    },
    {
        // 两边把「目标目录」写成了不同的参数名：照 NC 的 `target_parent_directory` 发到 SL 上，
        // SL 要的 `target_directory` 没填，会直接失败 —— 目录行上带「参数不同」徽章的典型
        name: 'move_group_file',
        summary: '移动群文件',
        category: 'file',
        safety: 'side_effect',
        params: [
            gid(),
            { name: 'file_id', type: 'string', desc: '文件 ID', required: true },
            { name: 'current_parent_directory', type: 'string', desc: '当前目录', required: true },
            {
                name: 'target_parent_directory',
                type: 'string',
                desc: '目标目录',
                required: true,
                only: 'napcat',
            },
            {
                name: 'target_directory',
                type: 'string',
                desc: '目标目录',
                required: true,
                only: 'snowluma',
            },
        ],
        returns: T.nul,
        example: {
            group_id: '100001',
            file_id: '/abcd-1234',
            current_parent_directory: '/',
            target_parent_directory: '/docs',
        },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '文件不存在' }, ...COMMON_ERRORS],
    },

    // —— 表情
    {
        name: 'fetch_custom_face',
        summary: '获取收藏的自定义表情',
        category: 'face',
        safety: 'read_only',
        params: [
            {
                name: 'count',
                type: 'integer',
                desc: '最多返回几个',
                default: 48,
                minimum: 1,
                maximum: 200,
            },
        ],
        returns: arr(T.str),
        returnData: ['https://example.invalid/face/1.png'],
        errorExamples: COMMON_ERRORS,
    },

    // —— 流式
    {
        name: 'upload_file_stream',
        summary: '分片上传文件（流式）',
        description:
            '把大文件切片后逐片发给 Bot，再由 Bot 合并落盘。本期调试台只提供文档，调用按钮置灰。',
        category: 'stream',
        safety: 'side_effect',
        stream: true,
        params: [
            {
                name: 'stream_id',
                type: 'string',
                desc: '同一个文件的分片共用一个 ID',
                required: true,
            },
            { name: 'chunk_data', type: 'string', desc: '本片内容（base64）' },
            { name: 'chunk_index', type: 'integer', desc: '本片序号，从 0 起' },
            { name: 'total_chunks', type: 'integer', desc: '总片数' },
            { name: 'file_size', type: 'integer', desc: '文件总字节数' },
            { name: 'filename', type: 'string', desc: '落盘文件名' },
        ],
        returns: obj({ type: T.str, stream_id: T.str, status: T.str, received_chunks: T.int }),
        returnData: {
            type: 'stream',
            stream_id: 's1',
            status: 'chunk_received',
            received_chunks: 1,
        },
        errorExamples: COMMON_ERRORS,
    },

    // —— 扩展（各自独有）
    {
        name: 'nc_get_rkey',
        summary: '获取图片直链所需的 rkey',
        category: 'extension',
        safety: 'read_only',
        backends: ['napcat'],
        params: [],
        returns: arr(obj({ type: T.str, rkey: T.str, created_at: T.int, ttl: T.int })),
        returnData: [
            { type: 'private', rkey: '&rkey=CAQSKAB6JW...', created_at: 1759190400, ttl: 86400 },
        ],
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'get_group_album_list',
        summary: '获取群相册列表',
        category: 'extension',
        safety: 'read_only',
        backends: ['snowluma'],
        params: [gid()],
        returns: arr(obj({ album_id: T.str, name: T.str, upload_number: T.int })),
        returnsText: '数组：每项含 album_id、name、upload_number',
        returnData: [{ album_id: 'album_1', name: '聚会', upload_number: 12 }],
        invariants: ['Bot 必须在这个群里'],
    },

    // —— 预览专用：真上游没有这个动作，只为在浏览器里走一遍超大回包被截断的界面
    {
        name: 'debug_huge_response',
        summary: '预览用：回一个超过 5 MiB 的回包',
        description:
            '浏览器预览专用，真 Bot 上没有这个动作。回包约 5.7 MiB，超过调试台 5 MiB 的内联上限：结果区只显示前 256 KiB 的文本预览，完整内容用「另存完整内容」保存。',
        category: 'extension',
        safety: 'read_only',
        params: [],
        returns: obj({
            note: T.str,
            rows: arr(
                obj({ seq: T.int, group_id: T.int, user_id: T.int, nickname: T.str, text: T.str }),
            ),
        }),
        returnsText: '对象：note，以及约五万行的 rows',
        returnData: {
            note: '预览用的超大回包：结果区只显示前 256 KiB，完整内容请另存',
            rows: [
                {
                    seq: 1,
                    group_id: 100001,
                    user_id: 10001,
                    nickname: '小明',
                    text: '第 1 行：预览用的填充数据',
                },
            ],
        },
        errorExamples: COMMON_ERRORS,
        invariants: ['只在浏览器预览里有', '回包超过 5 MiB，调试台只给前 256 KiB 的预览'],
    },
];

// ---------------------------------------------------------------------------
// 生成
// ---------------------------------------------------------------------------

const otherOf = (backend: BackendType): BackendType =>
    backend === 'napcat' ? 'snowluma' : 'napcat';
const hasBackend = (def: ActionDef, backend: BackendType) =>
    !def.backends || def.backends.includes(backend);
const isRequired = (p: ParamDef, backend: BackendType) =>
    p.requiredOn?.[backend] ?? p.required === true;
const visibleParams = (def: ActionDef, backend: BackendType) =>
    def.params.filter((p) => !p.only || p.only === backend);

function propertySchema(p: ParamDef, backend: BackendType): Record<string, unknown> {
    const type = p.typeOn?.[backend] ?? p.type;
    let schema: Record<string, unknown>;
    if (p.role && ID_ROLES.has(p.role)) {
        // NapCat 的 id 一律按字符串收，SnowLuma 收整数
        schema = backend === 'napcat' ? { type: 'string' } : { type: 'integer' };
    } else if (p.role === 'message') {
        schema = {
            ...(backend === 'napcat' ? { $id: 'OB11MessageMixType' } : {}),
            anyOf: [{ type: 'array', items: { type: 'object' } }, { type: 'string' }],
        };
    } else if (p.values) {
        schema = { type: typeof p.values[0] === 'number' ? 'integer' : 'string', enum: p.values };
    } else if (type === 'boolean') {
        // TypeBox 生成的布尔参数：NapCat 同时接受字符串 "true" / "false"
        schema =
            backend === 'napcat'
                ? { anyOf: [{ type: 'boolean' }, { type: 'string' }] }
                : { type: 'boolean' };
    } else if (type === 'array') {
        schema = { type: 'array', items: { type: 'object' } };
    } else if (type === 'object') {
        schema = { type: 'object' };
    } else {
        schema = { type: type ?? 'string' };
    }
    if (p.role) schema['x-ncd-role'] = p.role;
    schema.description = p.desc;
    if (p.default !== undefined) schema.default = p.default;
    if (p.minimum !== undefined) schema.minimum = p.minimum;
    if (p.maximum !== undefined) schema.maximum = p.maximum;
    return schema;
}

function paramsSchema(def: ActionDef, backend: BackendType): Record<string, unknown> {
    const params = visibleParams(def, backend);
    const required = params.filter((p) => isRequired(p, backend)).map((p) => p.name);
    return {
        type: 'object',
        properties: Object.fromEntries(params.map((p) => [p.name, propertySchema(p, backend)])),
        ...(required.length > 0 ? { required } : {}),
    };
}

/** 给人看的类型写法 */
function typeText(schema: Record<string, unknown>): string {
    if (typeof schema.type === 'string') return schema.type;
    const any = schema.anyOf;
    if (Array.isArray(any)) {
        return any.map((s) => String((s as Record<string, unknown>).type ?? 'any')).join(' | ');
    }
    return 'any';
}

/**
 * 比较用的类型。和后端转换器同一口径：id 类的字符串 / 整数算同一种；布尔的 `boolean` 和
 * `boolean|string` 写法算同一种；`integer` 是 `number` 的子集，比较前并成同一个
 * （NapCat 把 `duration` 写成 number，SnowLuma 写成 integer，这不是差异）
 */
function comparableType(schema: Record<string, unknown>): string {
    const role = schema['x-ncd-role'];
    if (typeof role === 'string' && ID_ROLES.has(role)) return 'id';
    if (role === 'message') return 'message';
    const any = schema.anyOf;
    if (Array.isArray(any) && any.some((s) => (s as Record<string, unknown>).type === 'boolean'))
        return 'boolean';
    return typeText(schema).replace(/\binteger\b/g, 'number');
}

/** 真正必须由调用方给出的参数：列在 required 里且没写 default（NapCat 会把带默认值的也列进去） */
function requiredWithoutDefault(
    props: Record<string, Record<string, unknown>>,
    required: Set<string>,
): Set<string> {
    const out = new Set<string>();
    for (const name of required) {
        const prop = props[name];
        if (prop !== undefined && prop.default === undefined) out.add(name);
    }
    return out;
}

/**
 * 两边同名参数的出入，以及「照一边的写法发到另一边会不会直接失败」。
 * `breaking` 的口径和 Rust 的 `params_incompatible` 一致：同名参数类型大类不同，
 * 或者某一侧必填、另一侧根本没有这个参数。只是必填与否不同、或只有一侧有的可选参数不算。
 */
function diffParams(
    def: ActionDef,
    here: BackendType,
): { diffs: DebugParamDiff[]; breaking: boolean } {
    const there = otherOf(here);
    const a = paramsSchema(def, here);
    const b = paramsSchema(def, there);
    const propsA = (a.properties ?? {}) as Record<string, Record<string, unknown>>;
    const propsB = (b.properties ?? {}) as Record<string, Record<string, unknown>>;
    const reqA = new Set((a.required ?? []) as string[]);
    const reqB = new Set((b.required ?? []) as string[]);
    const strictA = requiredWithoutDefault(propsA, reqA);
    const strictB = requiredWithoutDefault(propsB, reqB);
    const diffs: DebugParamDiff[] = [];
    let typeClash = false;
    for (const name of Object.keys(propsA)) {
        const other = propsB[name];
        const mine = propsA[name] as Record<string, unknown>;
        if (!other) {
            diffs.push({ name, diff: { kind: 'only_here' } });
        } else if (comparableType(mine) !== comparableType(other)) {
            typeClash = true;
            diffs.push({
                name,
                diff: { kind: 'type_differs', here: typeText(mine), other: typeText(other) },
            });
        } else if (reqA.has(name) !== reqB.has(name)) {
            diffs.push({
                name,
                diff: { kind: 'required_differs', here: reqA.has(name), other: reqB.has(name) },
            });
        }
    }
    for (const name of Object.keys(propsB)) {
        if (!propsA[name]) diffs.push({ name, diff: { kind: 'only_other' } });
    }
    const lacksRequired =
        [...strictA].some((name) => propsB[name] === undefined) ||
        [...strictB].some((name) => propsA[name] === undefined);
    return { diffs, breaking: typeClash || lacksRequired };
}

function buildBase(def: ActionDef, backend: BackendType): DebugActionSpec {
    const napcat = backend === 'napcat';
    const otherPresent = hasBackend(def, otherOf(backend));
    // 对方没有这个接口时按「没有」算：diff 为空，也不叫不兼容
    const diff = otherPresent ? diffParams(def, backend) : { diffs: [], breaking: false };
    return {
        name: def.name,
        aliases: def.aliases ?? [],
        summary: def.summary,
        description: napcat ? (def.description ?? null) : null,
        category: def.category,
        safety: def.safety,
        stream: def.stream === true,
        supported: true,
        params_schema: paramsSchema(def, backend),
        // SnowLuma 的 `returns` 常常只是一段话；写了 returnsText 的动作它就不给结构
        returns_schema: def.returns && (napcat || !def.returnsText) ? def.returns : null,
        returns_text: !napcat ? (def.returnsText ?? null) : null,
        return_example: napcat && def.returnData !== undefined ? reply(def.returnData) : null,
        examples: napcat && def.example ? [def.example] : [],
        error_examples: napcat ? (def.errorExamples ?? []) : [],
        invariants: napcat ? [] : (def.invariants ?? []),
        other_backend: {
            backend: otherOf(backend),
            present: otherPresent,
            diffs: diff.diffs,
            breaking: diff.breaking,
        },
        source: 'snapshot',
    };
}

const baseCache = new Map<BackendType, DebugActionSpec[]>();

function baseSpecs(backend: BackendType): DebugActionSpec[] {
    let list = baseCache.get(backend);
    if (!list) {
        list = DEFS.filter((d) => hasBackend(d, backend)).map((d) => buildBase(d, backend));
        baseCache.set(backend, list);
    }
    return list;
}

export interface MockSpecOptions {
    source: DebugCatalogSource;
    /** 当前 Bot 没实现的动作名（老版本上游缺的），进「当前 Bot 不支持」分组 */
    unsupported?: ReadonlySet<string>;
}

/** 按名字或别名取一个动作的完整说明；这个后端没有就返回 null */
export function buildMockSpec(
    backend: BackendType,
    nameOrAlias: string,
    opts: MockSpecOptions,
): DebugActionSpec | null {
    const base = baseSpecs(backend).find(
        (s) => s.name === nameOrAlias || s.aliases.includes(nameOrAlias),
    );
    if (!base) return null;
    return { ...base, supported: !opts.unsupported?.has(base.name), source: opts.source };
}

function summarize(spec: DebugActionSpec): DebugActionSummary {
    return {
        name: spec.name,
        aliases: spec.aliases,
        summary: spec.summary,
        category: spec.category,
        safety: spec.safety,
        stream: spec.stream,
        supported: spec.supported,
        other_backend_present: spec.other_backend ? spec.other_backend.present : null,
        // 只有真的不兼容才带徽章；只是必填不同、或只有一侧有的可选参数，只在文档页的对照表里列
        param_diff: spec.other_backend?.breaking ?? false,
    };
}

export function buildMockCatalog(backend: BackendType, opts: MockSpecOptions): DebugCatalog {
    return {
        backend,
        source: opts.source,
        snapshot_version: backend === 'napcat' ? '4.15.18' : '0.9.0',
        actions: baseSpecs(backend).map((base) =>
            summarize({
                ...base,
                supported: !opts.unsupported?.has(base.name),
                source: opts.source,
            }),
        ),
    };
}

/** 一个动作在这个后端上的必填参数名；动作不存在返回 null */
export function mockRequiredParams(backend: BackendType, name: string): string[] | null {
    const def = DEFS.find((d) => d.name === name && hasBackend(d, backend));
    if (!def) return null;
    return visibleParams(def, backend)
        .filter((p) => isRequired(p, backend))
        .map((p) => p.name);
}

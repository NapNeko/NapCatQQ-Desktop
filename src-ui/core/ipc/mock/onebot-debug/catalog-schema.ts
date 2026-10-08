// 接口目录的公共定义：动作/参数结构的类型，和参数、返回结构、错误示例的小零件。

import type { BackendType } from '../../generated/domain/BackendType';
import type { DebugActionCategory } from '../../generated/debug/DebugActionCategory';
import type { DebugActionSafety } from '../../generated/debug/DebugActionSafety';
import type { DebugErrorExample } from '../../generated/debug/DebugErrorExample';

export type Role =
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

export const ID_ROLES: ReadonlySet<string> = new Set([
    'group_id',
    'user_id',
    'member_id',
    'message_id',
]);

export interface ParamDef {
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

export interface ActionDef {
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

export const gid = (extra: Partial<ParamDef> = {}): ParamDef => ({
    name: 'group_id',
    role: 'group_id',
    desc: '群号',
    required: true,
    ...extra,
});
export const uid = (extra: Partial<ParamDef> = {}): ParamDef => ({
    name: 'user_id',
    role: 'user_id',
    desc: 'QQ 号',
    required: true,
    ...extra,
});
export const member = (desc = '群成员的 QQ 号'): ParamDef => ({
    name: 'user_id',
    role: 'member_id',
    desc,
    required: true,
});
export const message = (): ParamDef => ({
    name: 'message',
    role: 'message',
    desc: '要发送的内容，消息段数组或 CQ 码文本',
    required: true,
});
export const noCache = (extra: Partial<ParamDef> = {}): ParamDef => ({
    name: 'no_cache',
    type: 'boolean',
    desc: '不使用缓存',
    default: false,
    ...extra,
});
export const autoEscape = (): ParamDef => ({
    name: 'auto_escape',
    type: 'boolean',
    desc: '消息内容当作纯文本发送，不解析 CQ 码',
    default: false,
});

export const T = {
    int: { type: 'integer' },
    str: { type: 'string' },
    bool: { type: 'boolean' },
    nul: { type: 'null' },
} as const;
export const obj = (properties: Record<string, unknown>) => ({ type: 'object', properties });
export const arr = (items: unknown) => ({ type: 'array', items });

export const GROUP_ROW = obj({
    group_id: T.int,
    group_name: T.str,
    member_count: T.int,
    max_member_count: T.int,
    group_all_shut: T.int,
    group_remark: T.str,
});
export const MEMBER_ROW = obj({
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
export const MESSAGE_ID_ROW = obj({ message_id: T.int });

/** 样例回包的外壳：NapCat 的示例是完整的 OB11 回复 */
export const reply = (data: unknown) => ({
    status: 'ok',
    retcode: 0,
    data,
    message: '',
    wording: '',
    echo: null,
});

export const COMMON_ERRORS: DebugErrorExample[] = [
    { retcode: 1400, message: '请求参数错误' },
    { retcode: 1404, message: '不支持的 API' },
];
export const SEND_ERRORS: DebugErrorExample[] = [
    { retcode: 1200, message: '消息发送失败：群不存在或未加入' },
    { retcode: 1400, message: '请求参数错误' },
];

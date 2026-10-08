// 调用回包处理器：每个动作按真实上游的形状造数据；未收录的动作在调用侧兜底成 GENERIC_OK。

import { HUGE_RESPONSE_ROWS } from './consts';
import { pushOb11, nextMessageId, type BotState } from './state';
import {
    BIG_GROUP_MEMBER_COUNT,
    FRIEND_COUNT,
    GROUP_COUNT,
    GROUP_ID_BASE,
    USER_ID_BASE,
    groupNameOf,
    isKnownGroup,
    makeActionNotice,
    makeSelfMessage,
    memberCountOf,
    messageToSegments,
    personAt,
    placeholderImage,
    type MockPerson,
    type Ob11Segment,
} from '../onebot-debug-events.mock';

// ---------------------------------------------------------------------------
// 调用
// ---------------------------------------------------------------------------

export interface Reply {
    retcode: number;
    data: unknown;
    message?: string;
    wording?: string;
}

export interface HandlerCtx {
    st: BotState;
    params: Record<string, unknown>;
}

export type Handler = (c: HandlerCtx) => Reply;

export const okReply = (data: unknown = null): Reply => ({ retcode: 0, data });
export const failReply = (retcode: number, wording: string): Reply => ({
    retcode,
    data: null,
    message: wording,
    wording,
});

export const numParam = (params: Record<string, unknown>, key: string): number => {
    const v = params[key];
    if (typeof v === 'number') return v;
    if (typeof v === 'string' && v.trim() !== '') return Number(v);
    return Number.NaN;
};

export function groupRow(groupId: number) {
    return {
        group_id: groupId,
        group_name: groupNameOf(groupId),
        member_count: memberCountOf(groupId),
        max_member_count: groupId === GROUP_ID_BASE + 1 ? BIG_GROUP_MEMBER_COUNT : 200,
        group_all_shut: 0,
        group_remark: '',
    };
}

export const memberCache = new Map<number, Array<Record<string, unknown>>>();

export function membersOf(groupId: number): Array<Record<string, unknown>> {
    let list = memberCache.get(groupId);
    if (!list) {
        list = Array.from({ length: memberCountOf(groupId) }, (_, i) => {
            const p = personAt(i + 1);
            return {
                group_id: groupId,
                user_id: p.id,
                nickname: p.nickname,
                card: p.card,
                sex: 'unknown',
                age: 0,
                area: '',
                level: String(1 + ((i * 3) % 60)),
                qq_level: 0,
                join_time: 1_650_000_000 + i * 86_400,
                last_sent_time: 1_759_000_000 - i * 600,
                title_expire_time: 0,
                unfriendly: false,
                card_changed: false,
                is_robot: false,
                shut_up_timestamp: 0,
                role: p.role,
                title: '',
            };
        });
        memberCache.set(groupId, list);
    }
    return list;
}

export function friendRow(p: MockPerson) {
    return {
        user_id: p.id,
        nickname: p.nickname,
        remark: p.card,
        sex: 'unknown',
        age: 0,
        level: 0,
    };
}

export const knownGroup = (params: Record<string, unknown>): number | null => {
    const id = numParam(params, 'group_id');
    return isKnownGroup(id) ? id : null;
};

export function sendMessage(
    c: HandlerCtx,
    messageType: 'group' | 'private',
    targetId: number,
    segments: Ob11Segment[],
): Reply {
    if (segments.length === 0) return failReply(1400, 'message 不能为空');
    const messageId = nextMessageId();
    // 上游会把自己发的消息再报回来（message_sent），和调用回包分别到达，界面要按 message_id 合并
    pushOb11(
        c.st,
        makeSelfMessage(c.st.ctx, Date.now(), { messageId, messageType, targetId, segments }),
    );
    return okReply({ message_id: messageId });
}

export function pushNotice(c: HandlerCtx, notice: Record<string, unknown>): void {
    pushOb11(c.st, makeActionNotice(c.st.ctx, Date.now(), notice));
}

export const HANDLERS: Record<string, Handler> = {
    get_login_info: (c) => okReply({ user_id: c.st.bot.qq, nickname: c.st.bot.name }),
    get_status: (c) => okReply({ online: c.st.bot.online ?? true, good: true, stat: {} }),
    get_version_info: (c) =>
        okReply(
            c.st.bot.backend === 'napcat'
                ? { app_name: 'NapCat.Onebot', app_version: '4.15.18', protocol_version: 'v11' }
                : { app_name: 'SnowLuma', app_version: '0.9.0', protocol_version: 'v11' },
        ),
    bot_exit: () => okReply(),

    send_group_msg: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null)
            return failReply(1200, `群 ${String(c.params.group_id)} 不存在或没有加入`);
        return sendMessage(c, 'group', groupId, messageToSegments(c.params.message));
    },
    send_private_msg: (c) => {
        const userId = numParam(c.params, 'user_id');
        if (!Number.isFinite(userId)) return failReply(1400, 'user_id 不合法');
        return sendMessage(c, 'private', userId, messageToSegments(c.params.message));
    },
    send_msg: (c) => {
        const type =
            c.params.message_type ?? (c.params.group_id !== undefined ? 'group' : 'private');
        return type === 'group' ? HANDLERS.send_group_msg(c) : HANDLERS.send_private_msg(c);
    },
    send_group_forward_msg: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null)
            return failReply(1200, `群 ${String(c.params.group_id)} 不存在或没有加入`);
        const resId = `res_${nextMessageId().toString(16)}`;
        const reply = sendMessage(c, 'group', groupId, [{ type: 'forward', data: { id: resId } }]);
        return reply.retcode === 0 ? okReply({ ...(reply.data as object), res_id: resId }) : reply;
    },
    get_msg: (c) => {
        const found = c.st.messages.get(numParam(c.params, 'message_id'));
        if (!found) return failReply(1200, '消息不存在');
        return okReply({
            time: found.time,
            message_type: found.message_type,
            message_id: found.message_id,
            real_id: found.real_id,
            sender: found.sender,
            message: found.message,
            raw_message: found.raw_message,
            ...(found.group_id !== undefined ? { group_id: found.group_id } : {}),
        });
    },
    delete_msg: (c) => {
        const id = numParam(c.params, 'message_id');
        const found = c.st.messages.get(id);
        if (!found) return failReply(1200, '消息不存在或已超过撤回时限');
        c.st.messages.delete(id);
        pushNotice(
            c,
            found.message_type === 'group'
                ? {
                      notice_type: 'group_recall',
                      group_id: found.group_id,
                      user_id: found.user_id,
                      operator_id: c.st.bot.qq,
                      message_id: id,
                  }
                : { notice_type: 'friend_recall', user_id: found.user_id, message_id: id },
        );
        return okReply();
    },
    get_forward_msg: (c) => {
        const from = numParam(c.params, 'message_id');
        const sample = c.st.messages.get(from);
        const people = [personAt(1), personAt(2)];
        return okReply({
            messages: people.map((p, i) => ({
                sender: { user_id: p.id, nickname: p.nickname },
                time: 1_759_190_400 + i * 60,
                message:
                    sample && i === 0
                        ? sample.message
                        : [{ type: 'text', data: { text: `转发的第 ${i + 1} 条` } }],
            })),
        });
    },

    get_group_list: () =>
        okReply(Array.from({ length: GROUP_COUNT }, (_, i) => groupRow(GROUP_ID_BASE + i + 1))),
    get_group_info: (c) => {
        const groupId = knownGroup(c.params);
        return groupId === null ? failReply(1200, '群不存在') : okReply(groupRow(groupId));
    },
    get_group_member_list: (c) => {
        const groupId = knownGroup(c.params);
        return groupId === null ? failReply(1200, '群不存在') : okReply(membersOf(groupId));
    },
    get_group_member_info: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, '群不存在');
        const userId = numParam(c.params, 'user_id');
        const hit = membersOf(groupId).find((m) => m.user_id === userId);
        return hit ? okReply(hit) : failReply(1200, '群成员不存在');
    },
    set_group_ban: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, '群不存在');
        const duration = numParam(c.params, 'duration');
        const seconds = Number.isFinite(duration) ? duration : 1800;
        pushNotice(c, {
            notice_type: 'group_ban',
            sub_type: seconds > 0 ? 'ban' : 'lift_ban',
            group_id: groupId,
            operator_id: c.st.bot.qq,
            user_id: numParam(c.params, 'user_id'),
            duration: seconds,
        });
        return okReply();
    },
    set_group_kick: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, '群不存在');
        pushNotice(c, {
            notice_type: 'group_decrease',
            sub_type: 'kick',
            group_id: groupId,
            operator_id: c.st.bot.qq,
            user_id: numParam(c.params, 'user_id'),
        });
        return okReply();
    },
    set_group_whole_ban: (c) =>
        knownGroup(c.params) === null ? failReply(1200, '群不存在') : okReply(),
    set_group_card: (c) =>
        knownGroup(c.params) === null ? failReply(1200, '群不存在') : okReply(),

    get_friend_list: () =>
        okReply(Array.from({ length: FRIEND_COUNT }, (_, i) => friendRow(personAt(i + 1)))),
    get_stranger_info: (c) => {
        const userId = numParam(c.params, 'user_id');
        const n = userId - USER_ID_BASE;
        const person =
            n >= 1 && n <= 500 ? personAt(n) : { id: userId, nickname: `陌生人 ${userId}` };
        return okReply({
            user_id: person.id,
            nickname: person.nickname,
            sex: 'unknown',
            age: 0,
            qid: '',
            long_nick: '',
            reg_time: 1_500_000_000,
            is_vip: false,
        });
    },
    send_like: (c) => {
        const times = numParam(c.params, 'times');
        return Number.isFinite(times) && times > 20
            ? failReply(1200, '今日点赞次数已达上限')
            : okReply();
    },

    get_group_file_url: () => okReply({ url: 'https://example.invalid/group-file/abcd-1234' }),
    fetch_custom_face: (c) => {
        const count = numParam(c.params, 'count');
        const n = Math.min(Number.isFinite(count) ? count : 48, 12);
        return okReply(Array.from({ length: n }, (_, i) => placeholderImage(i + 1, 96, 96)));
    },
    nc_get_rkey: () =>
        okReply([
            {
                type: 'private',
                rkey: '&rkey=CAQSKAB6JWENi5LM',
                created_at: 1_759_190_400,
                ttl: 86_400,
            },
            {
                type: 'group',
                rkey: '&rkey=CAESKAB6JWENi5LM',
                created_at: 1_759_190_400,
                ttl: 86_400,
            },
        ]),
    get_group_album_list: (c) =>
        knownGroup(c.params) === null
            ? failReply(1200, '群不存在')
            : okReply([
                  { album_id: 'album_1', name: '聚会', upload_number: 12 },
                  { album_id: 'album_2', name: '截图', upload_number: 87 },
              ]),

    // 预览专用：一份约 5.7 MiB 的回包，走一遍「截断 → 只看预览 → 另存完整内容」。
    // 行是按序号算出来的，同样的调用永远回同样的内容；带中文，预览的截断点才会碰上多字节字符
    debug_huge_response: () =>
        okReply({
            note: '预览用的超大回包：结果区只显示前 256 KiB，完整内容请另存',
            rows: Array.from({ length: HUGE_RESPONSE_ROWS }, (_, i) => {
                const p = personAt((i % BIG_GROUP_MEMBER_COUNT) + 1);
                return {
                    seq: i + 1,
                    group_id: GROUP_ID_BASE + 1 + (i % GROUP_COUNT),
                    user_id: p.id,
                    nickname: p.nickname,
                    text: `第 ${i + 1} 行：预览用的填充数据`,
                };
            }),
        }),
};

/** 目录里有、但没写专门回包的动作（处理请求、上传删除文件等）：照成功回一个空结果 */
export const GENERIC_OK: Handler = () => okReply();

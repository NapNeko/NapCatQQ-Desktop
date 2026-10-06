// 资料投影只保留上游实际返回的字段。
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import { id, record, text, type Contact } from '../domain/chat/model';
import { callProblem } from '../domain/debug/errorCopy';
import { chatService } from './chat.service';

export interface ProfileField {
    label: string;
    value: string;
}
export interface ChatProfile {
    name: string;
    fields: ProfileField[];
}
export interface ProfileMember extends Contact {
    nickname: string;
    role: string;
    title: string;
    joined?: string;
    lastSent?: string;
}
function peer(target: DebugTarget, value: string): string | number {
    if (!/^[1-9]\d*$/.test(value) || !Number.isSafeInteger(Number(value)))
        throw new Error('无效的 QQ 号或群号');
    return target.backend === 'snowluma' ? Number(value) : value;
}
async function request(target: DebugTarget, action: string, params: unknown): Promise<unknown> {
    const response = await chatService.call(target.bot_id, action, params);
    const problem = callProblem(response);
    if (problem) throw new Error(problem);
    if (response.result.kind !== 'ok') throw new Error('资料读取失败');
    if (response.result.outcome.truncated) throw new Error('内容过大，请重试');
    return response.result.outcome.data;
}
const positive = (value: unknown): string =>
    typeof value === 'number' && Number.isFinite(value) && value > 0 ? String(value) : '';
const date = (value: unknown): string | undefined => {
    if (typeof value !== 'number' || value <= 0 || !Number.isFinite(value)) return;
    const parsed = new Date(value * 1000);
    return Number.isNaN(parsed.getTime()) ? undefined : parsed.toLocaleDateString('zh-CN');
};
export const chatProfileService = {
    async info(target: DebugTarget, contact: Contact): Promise<ChatProfile> {
        const group = contact.type === 'group';
        const raw = await request(
            target,
            group ? 'get_group_info' : 'get_stranger_info',
            group
                ? { group_id: peer(target, contact.id) }
                : { user_id: peer(target, contact.id), no_cache: false },
        );
        if (!raw || typeof raw !== 'object' || Array.isArray(raw))
            throw new Error('资料格式不正确');
        const row = record(raw);
        const fields: ProfileField[] = [];
        const add = (label: string, value: string | undefined) => {
            if (value) fields.push({ label, value });
        };
        if (group) {
            add('备注', text(row.group_remark));
            const count = positive(row.member_count);
            const max = positive(row.max_member_count);
            add('成员', count ? `${count}${max ? ` / ${max}` : ''} 人` : '');
            add('创建于', date(row.group_create_time));
            if (row.group_all_shut === -1 || row.group_all_shut === true) add('发言', '全员禁言中');
            return { name: text(row.group_name) || contact.name, fields };
        }
        add('昵称', text(row.nickname));
        add('性别', row.sex === 'male' ? '男' : row.sex === 'female' ? '女' : '');
        add('年龄', positive(row.age) ? `${positive(row.age)} 岁` : '');
        add('等级', positive(row.qqLevel) || positive(row.level));
        add('QID', text(row.qid));
        add('签名', text(row.long_nick));
        add('注册于', date(row.reg_time));
        return { name: text(row.remark) || text(row.nickname) || contact.name, fields };
    },
    async members(target: DebugTarget, groupId: string): Promise<ProfileMember[]> {
        const data = await request(target, 'get_group_member_list', {
            group_id: peer(target, groupId),
        });
        if (!Array.isArray(data)) throw new Error('成员列表格式不正确');
        const members: ProfileMember[] = [];
        const seen = new Set<string>();
        for (const value of data) {
            const row = record(value);
            const memberId = id(row.user_id);
            if (!/^[1-9]\d*$/.test(memberId) || seen.has(memberId)) continue;
            seen.add(memberId);
            members.push({
                key: `private:${memberId}`,
                type: 'private',
                id: memberId,
                name: text(row.card) || text(row.nickname) || memberId,
                nickname: text(row.nickname),
                role: text(row.role),
                title: text(row.title),
                joined: date(row.join_time),
                lastSent: date(row.last_sent_time),
            });
        }
        const rank = (role: string) => (role === 'owner' ? 0 : role === 'admin' ? 1 : 2);
        return members.sort(
            (a, b) => rank(a.role) - rank(b.role) || a.name.localeCompare(b.name, 'zh-CN'),
        );
    },
};

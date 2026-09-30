// 群 / 好友 / 群成员选择器的公共部分：候选来自 useDebugContacts（按 Bot 缓存 5 分钟，可以手动刷新）。
// 拉不到时不弹错误条，只在输入框下面写一句，用户直接填号就行。
//
// 群成员列表跟着表单里的 group_id 走，但不能跟着每一个按键走：敲「100001」的过程中会先后出现
// 1、10、100…… 每个都去拉一次成员列表，既慢又会把「群不存在」的失败留在缓存里。所以群号停手 400ms
// 后才生效，而且要像个完整的群号（在这个 Bot 的群列表里，或者至少 5 位数字）才去拉；0 和空的不拉。

import { useDebugContacts, type DebugContactKind } from '../../../../hooks/debug/useDebugContacts';
import type { DebugTarget } from '../../../../core/ipc/generated/debug/DebugTarget';
import { PickerCombo } from './PickerCombo';
import { useDebounced, type FieldProps } from './fieldKit';

const NOUN: Record<DebugContactKind, string> = { group: '群', friend: '好友', member: '群成员' };

export const GROUP_ID_SETTLE_MS = 400;
const MIN_GROUP_DIGITS = 5;

function groupIdText(v: unknown): string | null {
    if (typeof v === 'number' && Number.isFinite(v)) return String(v);
    if (typeof v === 'string' && /^\d+$/.test(v.trim())) return v.trim();
    return null;
}

export interface ContactPickerProps extends FieldProps {
    kind: DebugContactKind;
    target: DebugTarget | null;
    /** 群成员选择器跟着同一张表单里的 group_id 走 */
    groupId?: unknown;
}

export function ContactPicker(props: ContactPickerProps) {
    return props.kind === 'member' ? <MemberContactPicker {...props} /> : <PlainContactPicker {...props} />;
}

function PlainContactPicker({ kind, target, groupId: _groupId, ...field }: ContactPickerProps) {
    const contacts = useDebugContacts(target, kind);
    let unavailable: string | null = null;
    if (!target) unavailable = '先在顶栏选一个 Bot；也可以直接填号';
    else if (!target.running) unavailable = 'Bot 没在运行，列表拉不到，直接填号就行';
    return (
        <PickerCombo
            {...field}
            noun={NOUN[kind]}
            options={contacts.options}
            loading={contacts.isLoading}
            error={contacts.error}
            onRefresh={contacts.refresh}
            unavailable={unavailable}
            emptyText={`这个 Bot 上没有${NOUN[kind]}`}
        />
    );
}

function MemberContactPicker({ target, groupId, ...field }: ContactPickerProps) {
    const typed = groupIdText(groupId);
    const settled = useDebounced(typed, GROUP_ID_SETTLE_MS);
    // 群列表多半已经在缓存里（同一张表单上的群号字段就在用它），拿来判断填的群号是不是完整的
    const groups = useDebugContacts(target, 'group');
    const known = settled !== null && groups.options.some((g) => String(g.id) === settled);
    const complete = settled !== null && Number(settled) > 0 && (known || settled.length >= MIN_GROUP_DIGITS);
    const contacts = useDebugContacts(target, 'member', complete ? settled : null);

    let unavailable: string | null = null;
    if (!target) unavailable = '先在顶栏选一个 Bot；也可以直接填号';
    else if (!target.running) unavailable = 'Bot 没在运行，列表拉不到，直接填号就行';
    else if (typed === null || Number(typed) <= 0) unavailable = '先填 group_id，才能从群成员里挑';
    else if (typed === settled && !complete) unavailable = `群号「${typed}」看着还没填完，填完整才去拉成员列表`;

    // 群号还在变（停手等待中）且看着像完整的：列表先转圈，别把上一个群的成员当成这个群的
    const looksComplete = (id: string) => groups.options.some((g) => String(g.id) === id) || id.length >= MIN_GROUP_DIGITS;
    const waiting = typed !== null && typed !== settled && looksComplete(typed);
    return (
        <PickerCombo
            {...field}
            noun={NOUN.member}
            options={typed !== settled ? [] : contacts.options}
            loading={waiting || (typed === settled && contacts.isLoading)}
            error={typed !== settled ? null : contacts.error}
            onRefresh={contacts.refresh}
            unavailable={unavailable}
            emptyText="这个群里没有成员"
        />
    );
}

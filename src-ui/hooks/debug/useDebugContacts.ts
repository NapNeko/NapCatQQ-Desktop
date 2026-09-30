// 参数编辑器里「群号 / QQ 号 / 群成员」选择器的候选项：调一次 get_group_list 等只读接口，转成统一的选项。
//
// 拉不到不弹错误条：选择器会退化成普通输入框，用户直接填号就行，提示条只会添乱。

import { useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { debugErrorCopy } from '../../core/domain/debug/errorCopy';
import { newRequestId } from '../../core/domain/debug/ids';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import { useDebugChannelChoice } from './debugWorkspaceStore';
import { debugContactsKey, debugIdleKey } from './keys';

export type DebugContactKind = 'group' | 'friend' | 'member';

export interface DebugContactOption {
    id: number | string;
    label: string;
    hint?: string;
}

const EMPTY: DebugContactOption[] = [];
const AUTO: DebugChannelId = { kind: 'auto' };
const CONTACTS_TIMEOUT_MS = 15_000;
const STALE_MS = 5 * 60_000;

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** 上游的号有时是数字有时是数字串；认不出来的行直接跳过 */
function idOf(v: unknown): number | string | null {
    if (typeof v === 'number' && Number.isFinite(v)) return v;
    if (typeof v === 'string' && v.trim() !== '') return /^\d+$/.test(v.trim()) ? Number(v) : v.trim();
    return null;
}

const str = (v: unknown): string => (typeof v === 'string' ? v.trim() : '');

const ROLE_HINT: Record<string, string> = { owner: '群主', admin: '管理员' };

function toOption(kind: DebugContactKind, row: Record<string, unknown>): DebugContactOption | null {
    switch (kind) {
        case 'group': {
            const id = idOf(row.group_id);
            if (id === null) return null;
            const count = typeof row.member_count === 'number' ? ` · ${row.member_count} 人` : '';
            return { id, label: str(row.group_name) || String(id), hint: `${id}${count}` };
        }
        case 'friend': {
            const id = idOf(row.user_id);
            if (id === null) return null;
            const nickname = str(row.nickname);
            const remark = str(row.remark);
            return {
                id,
                label: remark || nickname || String(id),
                hint: remark && nickname ? `${nickname} · ${id}` : String(id),
            };
        }
        case 'member': {
            const id = idOf(row.user_id);
            if (id === null) return null;
            const nickname = str(row.nickname);
            const card = str(row.card);
            const label = card || nickname || String(id);
            const role = ROLE_HINT[str(row.role)];
            const parts = [card && nickname && card !== nickname ? nickname : '', String(id), role ?? ''].filter(Boolean);
            return { id, label, hint: parts.join(' · ') };
        }
    }
}

async function fetchContacts(
    botId: string,
    kind: DebugContactKind,
    groupId: number | null,
    channel: DebugChannelId,
): Promise<DebugContactOption[]> {
    const action =
        kind === 'group' ? 'get_group_list' : kind === 'friend' ? 'get_friend_list' : 'get_group_member_list';
    const response = await onebotDebugService.call({
        request_id: newRequestId(),
        bot_id: botId,
        channel,
        action,
        params: kind === 'member' ? { group_id: groupId } : {},
        timeout_ms: CONTACTS_TIMEOUT_MS,
        origin: 'picker',
    });

    if (response.result.kind === 'err') {
        const copy = debugErrorCopy(response.result.error);
        throw new Error(copy.detail ? `${copy.title}：${copy.detail}` : copy.title);
    }
    const outcome = response.result.outcome;
    if (!outcome.ok || !Array.isArray(outcome.data)) {
        throw new Error(outcome.wording || outcome.message || `上游返回 retcode ${outcome.retcode}`);
    }
    const options: DebugContactOption[] = [];
    for (const row of outcome.data) {
        if (!isRecord(row)) continue;
        const option = toOption(kind, row);
        if (option) options.push(option);
    }
    return options;
}

/** 成员选择器要先有群号；填的是数字（或数字串）才算有 */
function groupIdNumber(groupId: number | string | null | undefined): number | null {
    if (typeof groupId === 'number') return Number.isFinite(groupId) ? groupId : null;
    if (typeof groupId === 'string' && /^\d+$/.test(groupId.trim())) return Number(groupId.trim());
    return null;
}

export function useDebugContacts(
    target: { bot_id: string; running?: boolean } | null,
    kind: DebugContactKind,
    groupId?: number | string | null,
) {
    const botId = target?.bot_id ?? null;
    // 用当前选的调用通道；换通道不换候选项（同一个 Bot 的联系人不因通道而异），所以不进缓存键，只在请求发出时读最新值
    const channelRef = useRef<DebugChannelId>(AUTO);
    channelRef.current = useDebugChannelChoice(botId)?.call ?? AUTO;

    const group = kind === 'member' ? groupIdNumber(groupId) : null;
    const enabled = botId !== null && target?.running !== false && (kind !== 'member' || group !== null);

    const query = useQuery<DebugContactOption[], Error>({
        queryKey: enabled ? debugContactsKey(botId, kind, group) : debugIdleKey,
        queryFn: () => fetchContacts(botId ?? '', kind, group, channelRef.current),
        enabled,
        staleTime: STALE_MS,
        retry: false,
        // 失败时没有数据，窗口一聚焦就会重拉；每次都要等最多 15 秒，得不偿失，用户可以手动「刷新」
        refetchOnWindowFocus: false,
    });

    return {
        options: query.data ?? EMPTY,
        isLoading: enabled && query.isFetching && !query.data,
        error: enabled && query.error ? errorText(query.error) : null,
        refresh: () => {
            if (enabled) void query.refetch();
        },
    };
}

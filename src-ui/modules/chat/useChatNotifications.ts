// 本地忽略持久化到后台，隐藏会话与手动免打扰分别保存。
import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import { errorText } from '../../core/domain/errors';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { useChatNotice } from '../../hooks/chat/useChatNotice';

export function useChatNotifications(target: DebugTarget, localHidden: string[]) {
    const client = useQueryClient();
    const status = useQuery({
        queryKey: ['chat', 'desktop'],
        queryFn: chatDesktopService.status,
        refetchInterval: 5000,
    });
    const [error, setError] = useState('');
    const [busy, setBusy] = useState(false);
    const synced = useRef(false);
    const account = status.data?.accounts.find(
        (row) =>
            row.target.bot_id === target.bot_id && row.preference.selfId === String(target.qq_id),
    );
    useChatNotice(
        `notifications:${target.bot_id}:${target.qq_id}`,
        `${target.name} · 提醒设置未保存`,
        error,
    );
    const hidden = useMemo(
        () => new Set(account?.preference.hiddenGroups?.map((id) => `group:${id}`) ?? []),
        [account?.preference.hiddenGroups],
    );
    const ignored = useMemo(
        () => new Set(account?.preference.ignoredGroups ?? []),
        [account?.preference.ignoredGroups],
    );
    const qqMuted = useMemo(
        () => new Map(account?.groups?.map((group) => [group.groupId, group.qqMuted]) ?? []),
        [account?.groups],
    );
    const mute = async (id: string, value: boolean, hide = false) => {
        setBusy(true);
        setError('');
        try {
            await chatDesktopService.ignoreGroup(
                target.bot_id,
                String(target.qq_id),
                id,
                value,
                hide,
            );
            await client.invalidateQueries({ queryKey: ['chat', 'desktop'] });
        } catch (reason) {
            setError(errorText(reason));
        } finally {
            setBusy(false);
        }
    };
    useEffect(() => {
        if (!account || synced.current) return;
        synced.current = true;
        const missing = localHidden
            .filter((key) => key.startsWith('group:') && !hidden.has(key))
            .map((key) => key.slice(6));
        if (missing.length)
            void chatDesktopService
                .mergeHiddenGroups(target.bot_id, String(target.qq_id), missing)
                .then(() => client.invalidateQueries({ queryKey: ['chat', 'desktop'] }))
                .catch((reason) => setError(errorText(reason)));
    }, [account, localHidden, hidden, client, target.bot_id, target.qq_id]);
    return { ignored, hidden, qqMuted, busy, mute };
}

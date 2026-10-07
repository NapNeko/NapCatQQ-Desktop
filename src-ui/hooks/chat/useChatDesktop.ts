// 主窗口侧的聊天桌面包装：偏好/忽略群写入（带现状的 ['chat','desktop'] 失效链）、
// 视图状态读取、托盘账号切换、独立窗口开合。弹出窗自己的 handoff 桥在
// hooks/desktop/useChatPopoutBridge，两边互补不重叠。
// 桌面状态查询（未读/偏好/群禁言，账号设置弹窗与通知面板共用）也在本文件：
// 写入口的失效链和查询键保持同源，消费方只认这一个入口。

import { useCallback, useMemo } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { chatDesktopService, isChatPopoutWindow } from '../../core/services/chat-desktop.service';
import type { ChatAccountPreference } from '../../core/ipc/generated/chat/ChatAccountPreference';
import type { ChatTrayNavigation } from '../../core/ipc/generated/chat/ChatTrayNavigation';
import type { ChatViewState } from '../../core/ipc/generated/chat/ChatViewState';

// 查询键只在写入口与 status 查询之间共享，两处在同一文件，不导出。
const CHAT_DESKTOP_KEY = ['chat', 'desktop'] as const;

// 两个消费方的时机不同：账号设置弹窗仅打开时拉一次，通知面板 5 秒轮询，
// 参数由调用方传入保持与现状逐项一致。
export function useChatDesktopStatus(options?: { enabled?: boolean; refetchInterval?: number }) {
    return useQuery({
        queryKey: CHAT_DESKTOP_KEY,
        queryFn: chatDesktopService.status,
        enabled: options?.enabled,
        refetchInterval: options?.refetchInterval,
    });
}

// popout 判定在窗口入口（startChat）落定，早于任何渲染，直读 service 的模块级标记；
// modules 层不许 import service，从这里走合规出口。
export { isChatPopoutWindow };

export function useChatDesktop() {
    const client = useQueryClient();

    // 现状三个写入口都是「service 成功后 invalidate ['chat','desktop']，失败原样抛」，
    // 链放进 mutationFn 让 await mutateAsync 与调用点 await service + await invalidate 等价。
    const preferenceMutation = useMutation({
        mutationFn: async (preference: ChatAccountPreference) => {
            await chatDesktopService.setPreference(preference);
            await client.invalidateQueries({ queryKey: CHAT_DESKTOP_KEY });
        },
    });
    const ignoreMutation = useMutation({
        mutationFn: async (args: [string, string, string, boolean, boolean]) => {
            await chatDesktopService.ignoreGroup(...args);
            await client.invalidateQueries({ queryKey: CHAT_DESKTOP_KEY });
        },
    });
    const mergeMutation = useMutation({
        mutationFn: async (args: [string, string, string[]]) => {
            await chatDesktopService.mergeHiddenGroups(...args);
            await client.invalidateQueries({ queryKey: CHAT_DESKTOP_KEY });
        },
    });
    const { mutateAsync: setPreferenceAsync } = preferenceMutation;
    const { mutateAsync: ignoreGroupAsync } = ignoreMutation;
    const { mutateAsync: mergeHiddenGroupsAsync } = mergeMutation;

    const setPreference = useCallback(
        (preference: ChatAccountPreference) => setPreferenceAsync(preference),
        [setPreferenceAsync],
    );
    const ignoreGroup = useCallback(
        (botId: string, selfId: string, groupId: string, ignored: boolean, hidden = false) =>
            ignoreGroupAsync([botId, selfId, groupId, ignored, hidden]),
        [ignoreGroupAsync],
    );
    const mergeHiddenGroups = useCallback(
        (botId: string, selfId: string, groups: string[]) =>
            mergeHiddenGroupsAsync([botId, selfId, groups]),
        [mergeHiddenGroupsAsync],
    );

    const loadView = useCallback(
        (claim = false): Promise<ChatViewState> => chatDesktopService.loadView(claim),
        [],
    );
    const takeTrayNavigation = useCallback(
        (): Promise<ChatTrayNavigation | null> => chatDesktopService.takeTrayNavigation(),
        [],
    );
    const onAccountSelected = useCallback(
        (cb: () => void) => chatDesktopService.onAccountSelected(cb),
        [],
    );
    const selectAccount = useCallback(
        (botId: string) => chatDesktopService.selectAccount(botId),
        [],
    );
    const open = useCallback((botId?: string) => chatDesktopService.open(botId), []);
    const close = useCallback((embed: boolean) => chatDesktopService.close(embed), []);

    // 全部稳定引用：ChatPage 的订阅 effect 依赖数组里带上它们也不会反复重订阅
    return useMemo(
        () => ({
            setPreference,
            ignoreGroup,
            mergeHiddenGroups,
            loadView,
            takeTrayNavigation,
            onAccountSelected,
            selectAccount,
            open,
            close,
        }),
        [
            setPreference,
            ignoreGroup,
            mergeHiddenGroups,
            loadView,
            takeTrayNavigation,
            onAccountSelected,
            selectAccount,
            open,
            close,
        ],
    );
}

// 主窗与独立窗口的桥：聊天弹出窗的交接 / 收回、调试弹出窗关闭后的作废、
// 聊天与调试入口对已开弹出窗的让位。事件订阅与弹出窗 IPC 收在此处，
// 路由壳只给路由切换动作（订阅在挂载期，动作全部稳定）。

import { useEffect, useMemo, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import { debugWindowService } from '../../core/services/debug-window.service';
import { windowEventService } from '../../core/services/desktop.service';
import { markWorkspaceStale } from '../debug/debugWorkspaceStore';
import type { AppRoute } from '../../shared/components/next/Sidebar';

export interface AppWindowBridgeHandlers {
    /** 当前路由；聊天交接要按「此刻是否在聊天页」决定先卸载聊天页还是原样准备 */
    route: AppRoute;
    /** 进聊天页并立即上屏（resume / embed 收回 / 交接失败的恢复回滚） */
    showChat: () => void;
    /** 进聊天页但不动可见性（启动时补做已错过的 embed，对齐原逻辑不强制淡入） */
    showChatQuiet: () => void;
    /** 回概览并上屏（聊天页要让位给弹出窗时的卸载过渡） */
    showOverview: () => void;
}

export function useAppWindowBridge({
    route,
    showChat,
    showChatQuiet,
    showOverview,
}: AppWindowBridgeHandlers): {
    focusChatIfOpen: () => Promise<boolean>;
    focusDebugIfOpen: () => Promise<boolean>;
} {
    const queryClient = useQueryClient();
    const routeRef = useRef(route);
    useEffect(() => {
        routeRef.current = route;
    }, [route]);

    useEffect(() => {
        let disposed = false;
        let resumeChat = false;
        const handoff = chatDesktopService.onRequest((request) => {
            if (disposed || request.v !== 1) return;
            if (request.action === 'resume') {
                if (resumeChat) showChat();
                return;
            }
            void (async () => {
                try {
                    // chatStore / react-dom 不在主窗首屏依赖图里：只有聊天交接发生的
                    // 那一刻才拉，静态引会把整个聊天 store 钉进主包（沿用原动态 import）
                    const { prepareChatHandoff, getChatSelectedBot } =
                        await import('../chat/chatStore');
                    const { flushSync } = await import('react-dom');
                    const wasChat = routeRef.current === 'chat';
                    resumeChat = wasChat;
                    try {
                        await prepareChatHandoff(
                            getChatSelectedBot(),
                            () => {
                                if (wasChat) flushSync(() => showOverview());
                            },
                            request.action === 'popout',
                        );
                        await chatDesktopService.reply(request.requestId, null);
                    } catch (error) {
                        if (wasChat) flushSync(() => showChat());
                        throw error;
                    }
                } catch (error) {
                    await chatDesktopService
                        .reply(request.requestId, String(error))
                        .catch(() => {});
                }
            })();
        });
        const embedded = chatDesktopService.onEmbedRequested(() => {
            void queryClient.invalidateQueries({ queryKey: ['chat'] });
            showChat();
        });
        void chatDesktopService.windowState().then((state) => {
            if (!disposed && state.embedRequested) showChatQuiet();
        });
        return () => {
            disposed = true;
            void handoff.then((un) => un());
            void embedded.then((un) => un());
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 订阅一次；动作与 queryClient 均稳定
    }, []);

    // 弹出窗关掉后，盘上的工作区 / 收藏可能已被它改写：把主窗里的内存副本作废，
    // 下次进调试页从盘上重读（此刻主窗的调试页必然没挂着）
    useEffect(() => {
        let cancelled = false;
        let unlisten: (() => void) | undefined;
        void windowEventService
            .onDebugPopoutClosed(() => {
                markWorkspaceStale();
                void queryClient.invalidateQueries();
            })
            .then((un) => {
                if (cancelled) {
                    un();
                    return;
                }
                unlisten = un;
            });
        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, [queryClient]);

    // service 方法是箭头属性，引用天然稳定；memo 一次让路由壳的 navigate 依赖不抖
    return useMemo(
        () => ({
            focusChatIfOpen: chatDesktopService.focusIfOpen,
            // 调试台弹出窗开着时主窗不进调试页（工作区 / 收藏落盘 JSON 是两窗同一份文件，
            // 两边同时写会互相盖），入口一律把弹出窗叫到前面；没开着才正常切路由
            focusDebugIfOpen: debugWindowService.focusIfOpen,
        }),
        [],
    );
}

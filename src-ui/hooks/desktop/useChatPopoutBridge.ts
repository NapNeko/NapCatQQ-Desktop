// 聊天弹出窗自己的 IPC 包装：窗口入口标记、handoff 回执、首帧 reveal、
// 回调主窗。effect 编排在 ChatPopoutApp（要绑它的本地 state），这里只收敛 service 引用。

import { useCallback, useMemo } from 'react';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import { trayService } from '../../core/services/desktop.service';
import type { ChatWindowRequest } from '../../core/ipc/generated/chat/ChatWindowRequest';

// markChatPopoutWindow 留在 service：它写模块级 popout 标记，service 自己
// （restoreReading 的位置恢复判定）也读同一份状态，挪进 hook 会两头各存一份。
export { markChatPopoutWindow } from '../../core/services/chat-desktop.service';

export function useChatPopoutBridge() {
    const onRequest = useCallback(
        (cb: (request: ChatWindowRequest) => void) => chatDesktopService.onRequest(cb),
        [],
    );
    const reply = useCallback(
        (requestId: string, error: string | null) => chatDesktopService.reply(requestId, error),
        [],
    );
    const reveal = useCallback(() => chatDesktopService.reveal(), []);
    const showMainWindow = useCallback(() => trayService.showMainWindow(), []);

    // 全部稳定引用：ChatPopoutApp 的订阅 effect 只在挂载时跑一次
    return useMemo(
        () => ({ onRequest, reply, reveal, showMainWindow }),
        [onRequest, reply, reveal, showMainWindow],
    );
}

// 聊天会话入口的账号清单查询（chat_targets）：ChatPage 每 15 秒轮询，选中项经
// reconcileChatAccounts 落到 chatStore。会话消息流由 chatStore 自己的订阅事件流维护，
// 不经 react-query，也不在这里重复挂失效：现状唯一的事件失效是弹出窗合并回主窗时
// hooks/desktop/useAppWindowBridge 对 ['chat'] 前缀的 invalidate，保持在那边不动。
import { useQuery } from '@tanstack/react-query';
import { chatService } from '../../core/services/chat.service';

export function useChatTargets() {
    return useQuery({
        queryKey: ['chat', 'targets'],
        queryFn: chatService.targets,
        refetchInterval: 15_000,
    });
}

// 主窗口内的原生双栏聊天。
import { useEffect, useState } from 'react';
import { MessagesSquare } from 'lucide-react';
import {
    reconcileChatAccounts,
    restoreChatView,
    selectChatBot,
    loadChatView,
} from '../../hooks/chat/chatStore';
import { useChatDesktop } from '../../hooks/chat/useChatDesktop';
import { useChatTargets } from '../../hooks/chat/useChatConversations';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import { accountKey } from '../../core/domain/chat/model';
import type { ChatTrayNavigation } from '../../core/ipc/generated/chat/ChatTrayNavigation';
import { cn } from '../../shared/utils/cn';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { BotPicker } from '../debug';
import { ChatWorkspace } from './ChatWorkspace';
import './chat.css';

let lastBot = '';
export function ChatPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
    const [selected, select] = useState(lastBot);
    const [restored, setRestored] = useState(false);
    const [navigation, setNavigation] = useState<ChatTrayNavigation | null>(null);
    const { loadView, takeTrayNavigation, onAccountSelected, selectAccount } = useChatDesktop();
    useEffect(() => {
        let alive = true;
        let ready = false;
        let queue = Promise.resolve();
        const synchronize = () => {
            queue = queue
                // 上一次同步失败不能把队列卡死，吞掉后续接
                .catch(() => {})
                .then(async () => {
                    if (!alive || !ready) return;
                    const [view, next] = await Promise.all([loadView(), takeTrayNavigation()]);
                    if (!alive) return;
                    if (next) setNavigation(next);
                    const bot = next?.botId ?? view.selectedBot;
                    if (bot) {
                        lastBot = bot;
                        selectChatBot(bot);
                        select(bot);
                    }
                });
            return queue;
        };
        const listening = onAccountSelected(() => {
            void synchronize().catch((error) => console.warn('chat tray navigation', error));
        });
        void loadChatView()
            .then(async (view) => {
                if (!alive) return;
                restoreChatView(view);
                await listening;
                ready = true;
                if (view.selectedBot) {
                    lastBot = view.selectedBot;
                    selectChatBot(view.selectedBot);
                    select(view.selectedBot);
                }
                await synchronize();
            })
            .catch((error) =>
                pushInfoBar({
                    key: 'chat:view-restore',
                    tone: 'danger',
                    title: '聊天视图恢复失败',
                    content: error instanceof Error ? error.message : String(error),
                }),
            )
            .finally(() => {
                if (alive) setRestored(true);
            });
        return () => {
            alive = false;
            void listening.then((unlisten) => unlisten());
        };
    }, [loadView, takeTrayNavigation, onAccountSelected]);
    const targets = useChatTargets();
    useEffect(() => {
        if (targets.data) reconcileChatAccounts(targets.data);
    }, [targets.data]);
    const target =
        targets.data?.find((t) => t.bot_id === selected) ??
        targets.data?.find((t) => t.running) ??
        targets.data?.[0];
    useEffect(() => {
        if (target && restored) {
            selectChatBot(target.bot_id);
            void selectAccount(target.bot_id).catch((error) =>
                console.warn('chat window account', error),
            );
        }
    }, [target?.bot_id, restored, selectAccount]);
    const picker = (connected: boolean, label: string) => (
        <BotPicker
            compact
            ariaLabel="聊天账号"
            statusIndicator={
                <span
                    role="img"
                    aria-label={label}
                    title={label}
                    className={cn(
                        'h-[7px] w-[7px] shrink-0 rounded-full',
                        connected ? 'bg-success' : 'bg-danger',
                    )}
                />
            }
            targets={targets.data ?? []}
            selected={target ?? null}
            loading={targets.isLoading}
            onSelect={(botId) => {
                lastBot = botId;
                selectChatBot(botId);
                select(botId);
            }}
            onManageBots={() => onNavigate('bots')}
        />
    );
    if (!restored)
        return (
            <section className="native-chat">
                <div className="native-chat-welcome">正在恢复聊天…</div>
            </section>
        );
    return (
        <section className="native-chat">
            {target ? (
                <ChatWorkspace
                    key={accountKey(target.bot_id, String(target.qq_id))}
                    target={target}
                    picker={picker}
                    onNavigate={onNavigate}
                    navigation={navigation}
                    onTrayHandled={() => setNavigation(null)}
                />
            ) : (
                <div className="native-chat-welcome">
                    <MessagesSquare size={36} strokeWidth={1.3} />
                    <h2>
                        {targets.isLoading
                            ? '正在读取账号'
                            : targets.isError
                              ? '账号读取失败'
                              : '从一个机器人开始聊天'}
                    </h2>
                    <button
                        className="native-chat-text-button"
                        onClick={() =>
                            targets.isError ? void targets.refetch() : onNavigate('bots')
                        }
                    >
                        {targets.isError ? '重试' : '前往机器人'}
                    </button>
                </div>
            )}
        </section>
    );
}

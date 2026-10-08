// 独立聊天窗只加载聊天工作区和共享桌面外观。
import { Suspense, lazy, useEffect, useState } from 'react';
import { flushSync } from 'react-dom';
import './index.css';
import { TitleBarChrome } from '../shared/components/next/TitleBarChrome';
import { TooltipProvider } from '../shared/ui/Tooltip';
import { RouteErrorBoundary } from '../shared/ui/RouteErrorBoundary';
import { prepareChatHandoff, getChatSelectedBot } from '../hooks/chat/chatStore';
import { errorText } from '../core/domain/errors';
import { InfoBarStack } from '../shared/ui/InfoBarStack';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { useChatPopoutBridge } from '../hooks/desktop/useChatPopoutBridge';
import { useChatNotice } from '../hooks/chat/useChatNotice';

const ChatPage = lazy(() =>
    import('../modules/chat/ChatPage').then((m) => ({ default: m.ChatPage })),
);
export function ChatPopoutApp() {
    const [handoffError, setHandoffError] = useState('');
    const [mounted, setMounted] = useState(true);
    const { bars, dismiss, remove } = useGlobalInfoBars();
    useChatNotice('window:handoff', '聊天窗口切换失败', handoffError);
    const { onRequest, reply, reveal, showMainWindow } = useChatPopoutBridge();
    useEffect(() => {
        let alive = true;
        const subscription = onRequest((request) => {
            if (!alive || request.v !== 1) return;
            if (request.action === 'resume') {
                setMounted(true);
                return;
            }
            setHandoffError('');
            void prepareChatHandoff(
                getChatSelectedBot(),
                () => flushSync(() => setMounted(false)),
                request.action === 'embed',
            )
                .then(() => reply(request.requestId, null))
                .catch((error) => {
                    const message = errorText(error);
                    setMounted(true);
                    setHandoffError(message);
                    return reply(request.requestId, message).catch(() => {});
                });
        });
        // 隐藏的 WebView 可能不派发动画帧；监听就绪后即可显示已提交的界面。
        void subscription
            .then(() => (alive ? reveal() : undefined))
            .catch((error) => {
                if (alive) setHandoffError(errorText(error));
            });
        return () => {
            alive = false;
            void subscription.then((unlisten) => unlisten()).catch(() => {});
        };
    }, [onRequest, reply, reveal]);
    return (
        <TooltipProvider>
            <div className="native-chat-popout flex h-screen flex-col overflow-hidden bg-canvas">
                <div className="relative shrink-0">
                    <TitleBarChrome tool />
                    <span className="native-chat-popout-title" aria-hidden>
                        聊天
                    </span>
                </div>
                <main className="flex min-h-0 flex-1 flex-col">
                    <RouteErrorBoundary title="聊天界面加载失败">
                        <Suspense
                            fallback={<div className="native-chat-welcome">正在加载聊天…</div>}
                        >
                            {mounted && <ChatPage onNavigate={() => void showMainWindow()} />}
                        </Suspense>
                    </RouteErrorBoundary>
                </main>
                <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />
            </div>
        </TooltipProvider>
    );
}

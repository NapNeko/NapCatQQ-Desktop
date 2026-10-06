// 独立聊天窗只加载聊天工作区和共享桌面外观。
import { Suspense, lazy, useEffect, useState } from 'react';
import { flushSync } from 'react-dom';
import './index.css';
import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { TooltipProvider } from '../shared/ui/Tooltip';
import { RouteErrorBoundary } from '../shared/ui/RouteErrorBoundary';
import { chatDesktopService } from '../core/services/chat-desktop.service';
import { prepareChatHandoff, getChatSelectedBot } from '../hooks/chat/chatStore';
import { trayService } from '../core/services/desktop.service';
import { errorText } from '../core/domain/errors';
import { InfoBarStack } from '../shared/ui/InfoBarStack';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { useChatNotice } from '../hooks/chat/useChatNotice';

const ChatPage = lazy(() =>
    import('../modules/chat/ChatPage').then((m) => ({ default: m.ChatPage })),
);
export function ChatPopoutApp() {
    const [handoffError, setHandoffError] = useState('');
    const [mounted, setMounted] = useState(true);
    const { bars, dismiss, remove } = useGlobalInfoBars();
    useChatNotice('window:handoff', '聊天窗口切换失败', handoffError);
    useEffect(() => {
        let alive = true;
        const subscription = chatDesktopService.onRequest((request) => {
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
                .then(() => chatDesktopService.reply(request.requestId, null))
                .catch((error) => {
                    const message = errorText(error);
                    setMounted(true);
                    setHandoffError(message);
                    return chatDesktopService.reply(request.requestId, message).catch(() => {});
                });
        });
        const frame = requestAnimationFrame(() =>
            requestAnimationFrame(() => {
                if (alive) void chatDesktopService.reveal();
            }),
        );
        return () => {
            alive = false;
            cancelAnimationFrame(frame);
            void subscription.then((unlisten) => unlisten());
        };
    }, []);
    return (
        <TooltipProvider>
            <div className="native-chat-popout flex h-screen flex-col overflow-hidden bg-canvas">
                <div className="relative shrink-0">
                    <CustomTitleBar variant="window" />
                    <span className="native-chat-popout-title" aria-hidden>
                        聊天
                    </span>
                </div>
                <main className="flex min-h-0 flex-1 flex-col">
                    <RouteErrorBoundary title="聊天界面加载失败">
                        <Suspense
                            fallback={<div className="native-chat-welcome">正在加载聊天…</div>}
                        >
                            {mounted && (
                                <ChatPage onNavigate={() => void trayService.showMainWindow()} />
                            )}
                        </Suspense>
                    </RouteErrorBoundary>
                </main>
                <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />
            </div>
        </TooltipProvider>
    );
}

// 聊天提示复用全局 InfoBar。恢复走 removeSoon 延迟移除：refetch 抖动不会闪出闪回。
import { useEffect, useRef } from 'react';
import { pushInfoBar, removeSoonInfoBar } from '../ui/globalInfoBarStore';
import { Button } from '../../shared/ui/Button';
import type { InfoBarTone } from '../../shared/ui/InfoBar';

export function useChatNotice(key: string, title: string, message: string | null | undefined, onRetry?: () => void, tone: InfoBarTone = 'danger') {
    const retry = useRef(onRetry);
    retry.current = onRetry;
    const canRetry = !!onRetry;
    useEffect(() => {
        if (!message) return;
        const id = pushInfoBar({
            key: `chat:${key}`,
            tone,
            title,
            content: message,
            children: canRetry ? <Button variant="ghost" size="sm" onClick={() => retry.current?.()}>重试</Button> : undefined,
        });
        return () => removeSoonInfoBar(id);
    }, [key, title, message, canRetry, tone]);
}

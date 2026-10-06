// 发送结果只暴露失败：成功/发送中不留痕；失败留一个红色感叹号，悬停看原因、点击重发（对齐 QQ）。
import { CircleAlert } from 'lucide-react';
import type { SendStatus } from '../../core/domain/chat/model';

export function ChatSendStatus({
    status,
    error,
    onRetry,
    retryDisabled,
}: {
    status: SendStatus;
    error?: string;
    onRetry?: () => void;
    retryDisabled?: boolean;
}) {
    if (status !== 'failed' && status !== 'unknown') return null;
    const retryable = status === 'failed' && !!onRetry && !retryDisabled;
    const label =
        status === 'failed'
            ? `发送失败${error ? `：${error}` : ''}${retryable ? '，点击重发' : ''}`
            : `发送结果未确认，请核实${error ? `：${error}` : ''}`;
    if (!retryable)
        return (
            <span
                className="native-chat-send-status"
                data-status={status}
                role="img"
                aria-label={label}
                title={label}
            >
                <CircleAlert size={14} />
            </span>
        );
    return (
        <button
            type="button"
            className="native-chat-send-status"
            data-status={status}
            aria-label={label}
            title={label}
            onClick={onRetry}
        >
            <CircleAlert size={14} />
        </button>
    );
}

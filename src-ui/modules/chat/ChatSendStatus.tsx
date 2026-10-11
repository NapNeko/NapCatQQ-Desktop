// 发送结果只暴露失败：成功/发送中不留痕；失败留一个红色感叹号，悬停看原因、点击重发（对齐 QQ）。
import { CircleAlert, Loader2 } from 'lucide-react';
import type { SendStatus } from '../../core/domain/chat/model';
import type { DebugStreamProgress } from '../../core/ipc/generated/debug/DebugStreamProgress';
import { fileSizeLabel } from '../../core/domain/debug/chatFormat';

export function ChatSendStatus({
    status,
    error,
    onRetry,
    retryDisabled,
    progress,
}: {
    status: SendStatus;
    error?: string;
    onRetry?: () => void;
    retryDisabled?: boolean;
    progress?: DebugStreamProgress;
}) {
    if (status === 'sending') {
        const transferring = progress && progress.stage !== 'calling';
        const percent =
            transferring && progress.total_bytes && progress.total_bytes > 0
                ? Math.min(100, Math.floor((progress.done_bytes / progress.total_bytes) * 100))
                : undefined;
        const label = transferring
            ? `${progress.stage === 'reading' ? '读取' : '上传'} ${progress.file_name} · ${percent === undefined ? fileSizeLabel(progress.done_bytes) : `${percent}%`}`
            : '正在发送…';
        return (
            <span className="native-chat-send-progress" role="status" title={label}>
                <Loader2 size={12} className="animate-spin" aria-hidden />
                <span>{label}</span>
                {transferring && <progress aria-label="附件发送进度" max={100} value={percent} />}
            </span>
        );
    }
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

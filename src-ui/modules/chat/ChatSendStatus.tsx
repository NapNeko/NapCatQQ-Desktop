// 快速发送不闪提示；较慢的发送在气泡旁显示进度，完成后淡出。
import { useEffect, useRef, useState } from 'react';
import { CircleAlert, X } from 'lucide-react';
import type { SendStatus } from '../../core/domain/chat/model';
import type { DebugStreamProgress } from '../../core/ipc/generated/debug/DebugStreamProgress';
import { fileSizeLabel } from '../../core/domain/debug/chatFormat';
import { ChatPresence } from './chatMotion';

export function ChatSendStatus({
    status,
    error,
    onRetry,
    retryDisabled,
    progress,
    onCancel,
}: {
    status: SendStatus;
    error?: string;
    onRetry?: () => void;
    retryDisabled?: boolean;
    progress?: DebugStreamProgress;
    onCancel?: () => Promise<void>;
}) {
    const [visible, setVisible] = useState(false);
    const currentStatus = useRef(status);
    const cancelInFlight = useRef('');
    const [cancelRequested, setCancelRequested] = useState('');
    currentStatus.current = status;
    useEffect(() => {
        if (status !== 'sending' || visible) return;
        const timer = window.setTimeout(() => setVisible(true), 240);
        return () => window.clearTimeout(timer);
    }, [status, visible]);
    const transferring = progress && progress.stage !== 'calling';
    const requestId = progress?.request_id ?? '';
    const cancelling = !!requestId && cancelRequested === requestId;
    const cancellable = status === 'sending' && !!transferring && !!onCancel;
    const cancel = async () => {
        if (!cancellable || !requestId || cancelInFlight.current === requestId) return;
        cancelInFlight.current = requestId;
        setCancelRequested(requestId);
        try {
            await onCancel!();
        } catch {
            if (cancelInFlight.current === requestId) {
                cancelInFlight.current = '';
                setCancelRequested('');
            }
        }
    };
    const percent =
        transferring && progress.total_bytes && progress.total_bytes > 0
            ? Math.max(
                  0,
                  Math.min(100, Math.floor((progress.done_bytes / progress.total_bytes) * 100)),
              )
            : undefined;
    const sendingLabel =
        cancelling && transferring
            ? '正在取消发送…'
            : transferring
              ? `${progress.stage === 'reading' ? '读取' : '上传'} ${progress.file_name} · ${percent === undefined ? fileSizeLabel(progress.done_bytes) : `${percent}%`}`
              : '正在发送…';
    const retryable = status === 'failed' && !!onRetry && !retryDisabled;
    const label =
        status === 'failed'
            ? `发送失败${error ? `：${error}` : ''}${retryable ? '，点击重发' : ''}`
            : `发送结果未确认，请核实${error ? `：${error}` : ''}`;
    const failed = status === 'failed' || status === 'unknown';
    return (
        <>
            {visible && (
                <ChatPresence
                    visible={status === 'sending'}
                    onExited={() => {
                        if (currentStatus.current !== 'sending') setVisible(false);
                    }}
                >
                    <span
                        className="native-chat-send-progress"
                        role="status"
                        aria-label={sendingLabel}
                        title={sendingLabel}
                    >
                        <span className="sr-only">{sendingLabel}</span>
                        {transferring ? (
                            <button
                                type="button"
                                className="native-chat-send-cancel"
                                data-cancellable={cancellable}
                                disabled={!cancellable || cancelling}
                                aria-label={
                                    cancelling ? '正在取消发送' : `取消发送 ${progress.file_name}`
                                }
                                title={
                                    cancelling
                                        ? '正在取消发送…'
                                        : cancellable
                                          ? '取消发送'
                                          : sendingLabel
                                }
                                onClick={() => void cancel()}
                            >
                                <span
                                    className="native-chat-send-ring"
                                    data-indeterminate={percent === undefined}
                                    role="progressbar"
                                    aria-label="附件发送进度"
                                    aria-valuemin={0}
                                    aria-valuemax={100}
                                    aria-valuenow={percent}
                                    aria-valuetext={sendingLabel}
                                >
                                    <svg width="26" height="26" viewBox="0 0 26 26" aria-hidden>
                                        <circle
                                            className="native-chat-send-ring-track"
                                            cx="13"
                                            cy="13"
                                            r="10"
                                        />
                                        <circle
                                            className="native-chat-send-ring-value"
                                            cx="13"
                                            cy="13"
                                            r="10"
                                            pathLength="100"
                                            strokeDasharray={
                                                percent === undefined ? '24 76' : '100 100'
                                            }
                                            strokeDashoffset={
                                                percent === undefined ? 0 : 100 - percent
                                            }
                                        />
                                    </svg>
                                    {percent !== undefined && <span aria-hidden>{percent}</span>}
                                    {cancellable && (
                                        <X
                                            className="native-chat-send-cancel-icon"
                                            size={12}
                                            aria-hidden
                                        />
                                    )}
                                </span>
                            </button>
                        ) : (
                            <span className="native-chat-send-dots" aria-hidden>
                                <i />
                                <i />
                                <i />
                            </span>
                        )}
                    </span>
                </ChatPresence>
            )}
            {failed &&
                (retryable ? (
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
                ) : (
                    <span
                        className="native-chat-send-status"
                        data-status={status}
                        role="img"
                        aria-label={label}
                        title={label}
                    >
                        <CircleAlert size={14} />
                    </span>
                ))}
        </>
    );
}

import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatSendStatus } from './ChatSendStatus';

describe('attachment sending status', () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    const reveal = () => act(() => vi.advanceTimersByTime(240));

    it('shows upload bytes as a percentage, then switches to server confirmation', () => {
        const progress = {
            v: 1,
            request_id: 'request',
            stage: 'uploading' as const,
            file_name: '报告.pdf',
            done_bytes: 40,
            total_bytes: 100,
            done_chunks: 1,
            total_chunks: 3,
        };
        const view = render(<ChatSendStatus status="sending" progress={progress} />);
        expect(screen.queryByRole('status')).not.toBeInTheDocument();
        reveal();
        expect(screen.getByRole('status')).toHaveTextContent('上传 报告.pdf · 40%');
        expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '40');
        view.rerender(
            <ChatSendStatus status="sending" progress={{ ...progress, stage: 'calling' }} />,
        );
        expect(screen.getByRole('status')).toHaveTextContent('正在发送');
        expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
        view.rerender(<ChatSendStatus status="sent" />);
        expect(screen.queryByRole('status')).not.toBeInTheDocument();
    });
    it('keeps unknown-size uploads indeterminate', () => {
        render(
            <ChatSendStatus
                status="sending"
                progress={{
                    v: 1,
                    request_id: 'request',
                    stage: 'uploading',
                    file_name: '报告.pdf',
                    done_bytes: 1024,
                    total_bytes: null,
                    done_chunks: 0,
                    total_chunks: null,
                }}
            />,
        );
        reveal();
        expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
        expect(screen.getByRole('status')).toHaveTextContent('上传 报告.pdf');
    });

    it('does not flash a status for fast sends', () => {
        const view = render(<ChatSendStatus status="sending" />);
        act(() => vi.advanceTimersByTime(120));
        view.rerender(<ChatSendStatus status="sent" />);
        reveal();
        expect(screen.queryByRole('status')).not.toBeInTheDocument();
    });

    it('cancels an upload once and stops offering cancellation during server confirmation', () => {
        const onCancel = vi.fn().mockResolvedValue(undefined);
        const progress = {
            v: 1,
            request_id: 'cancel-request',
            stage: 'uploading' as const,
            file_name: '文件.zip',
            done_bytes: 20,
            total_bytes: 100,
            done_chunks: 1,
            total_chunks: 5,
        };
        const view = render(
            <ChatSendStatus status="sending" progress={progress} onCancel={onCancel} />,
        );
        reveal();
        const cancel = screen.getByRole('button', { name: '取消发送 文件.zip' });
        fireEvent.click(cancel);
        fireEvent.click(cancel);
        expect(onCancel).toHaveBeenCalledOnce();
        expect(screen.getByRole('button', { name: '正在取消发送' })).toBeDisabled();
        view.rerender(
            <ChatSendStatus
                status="sending"
                progress={{ ...progress, stage: 'calling' }}
                onCancel={onCancel}
            />,
        );
        expect(screen.queryByRole('button')).not.toBeInTheDocument();
        expect(screen.getByRole('status')).toHaveAccessibleName('正在发送…');
    });

    it('updates byte progress without delaying an already visible indicator', () => {
        const progress = {
            v: 1,
            request_id: 'request',
            stage: 'reading' as const,
            file_name: '报告.pdf',
            done_bytes: 40,
            total_bytes: 100,
            done_chunks: 1,
            total_chunks: 3,
        };
        const view = render(<ChatSendStatus status="sending" progress={progress} />);
        reveal();
        view.rerender(
            <ChatSendStatus status="sending" progress={{ ...progress, done_bytes: 80 }} />,
        );
        expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '80');
        expect(screen.getByRole('status')).toHaveAttribute('title', '读取 报告.pdf · 80%');
    });

    it('offers retry for definitive failures while keeping unknown results non-retryable', () => {
        const onRetry = vi.fn();
        const view = render(
            <ChatSendStatus status="failed" error="连接已断开" onRetry={onRetry} />,
        );
        fireEvent.click(screen.getByRole('button', { name: /发送失败.*连接已断开.*点击重发/ }));
        expect(onRetry).toHaveBeenCalledOnce();
        view.rerender(<ChatSendStatus status="unknown" onRetry={onRetry} />);
        expect(screen.queryByRole('button')).not.toBeInTheDocument();
        expect(screen.getByRole('img')).toHaveAccessibleName(/发送结果未确认/);
    });
});

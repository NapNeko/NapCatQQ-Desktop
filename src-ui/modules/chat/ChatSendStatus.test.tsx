import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ChatSendStatus } from './ChatSendStatus';

describe('attachment sending status', () => {
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
        expect(screen.getByRole('status')).toHaveTextContent('上传 报告.pdf · 40%');
        expect(screen.getByRole('progressbar')).toHaveAttribute('value', '40');
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
        expect(screen.getByRole('progressbar')).not.toHaveAttribute('value');
        expect(screen.getByRole('status')).toHaveTextContent('上传 报告.pdf');
    });
});

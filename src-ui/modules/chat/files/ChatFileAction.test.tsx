import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Contact } from '../../../core/domain/chat/model';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { chatGroupFilesService } from '../../../core/services/chat-group-files.service';
import { ChatFileAction } from './ChatFileAction';

vi.mock('../../../core/services/chat-group-files.service', () => ({
    chatGroupFilesService: { download: vi.fn(), open: vi.fn(), cancel: vi.fn() },
}));

const target = { bot_id: 'file-actions', qq_id: 123, backend: 'snowluma' } as DebugTarget;
const contact = (type: 'group' | 'private'): Contact => ({
    key: `${type}:456`,
    type,
    id: '456',
    name: '文件会话',
});

describe('chat file downloads', () => {
    beforeEach(() => {
        vi.mocked(chatGroupFilesService.download).mockResolvedValue('/downloads/资料.zip');
        vi.mocked(chatGroupFilesService.open).mockResolvedValue();
    });

    it.each(['group', 'private'] as const)(
        'downloads a received legacy %s file and opens the saved result',
        async (type) => {
            render(
                <ChatFileAction
                    target={target}
                    contact={contact(type)}
                    data={{ id: `received-${type}`, name: '资料.zip', file_hash: 'file-hash' }}
                />,
            );
            fireEvent.click(screen.getByRole('button', { name: '下载' }));
            await screen.findByText('已下载');
            expect(chatGroupFilesService.download).toHaveBeenCalledWith(
                target,
                type === 'group'
                    ? { kind: 'group', groupId: '456', fileId: 'received-group', busid: null }
                    : {
                          kind: 'private',
                          userId: '456',
                          fileId: 'received-private',
                          fileHash: 'file-hash',
                      },
                '资料.zip',
                expect.any(String),
                expect.any(Function),
            );
            fireEvent.click(screen.getByRole('button', { name: '打开' }));
            expect(chatGroupFilesService.open).toHaveBeenCalledWith('/downloads/资料.zip', false);
        },
    );

    it('downloads URL-only messages through the same transfer flow and retries failures', async () => {
        vi.mocked(chatGroupFilesService.download).mockRejectedValueOnce(new Error('下载失败'));
        const url = 'https://cdn.example/url-only.zip';
        render(
            <ChatFileAction
                target={target}
                contact={contact('private')}
                data={{ name: '资料.zip', url }}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '下载' }));
        fireEvent.click(await screen.findByRole('button', { name: '重试' }));
        await screen.findByText('已下载');
        expect(chatGroupFilesService.download).toHaveBeenCalledTimes(2);
        expect(chatGroupFilesService.download).toHaveBeenLastCalledWith(
            target,
            { kind: 'url', url },
            '资料.zip',
            expect.any(String),
            expect.any(Function),
        );
    });

    it('leaves a cancelled save dialog ready to download again', async () => {
        vi.mocked(chatGroupFilesService.download).mockResolvedValueOnce(null);
        render(
            <ChatFileAction
                target={target}
                contact={contact('private')}
                data={{ file_id: 'cancelled-dialog', file: '资料.zip' }}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '下载' }));
        await waitFor(() => expect(screen.getByRole('button', { name: '下载' })).toBeEnabled());
        expect(screen.queryByText('已下载')).not.toBeInTheDocument();
    });

    it('passes a safe basename when the only file field is a signed URL', async () => {
        const url = 'https://cdn.example/report%3Afinal.pdf?token=secret';
        render(
            <ChatFileAction target={target} contact={contact('private')} data={{ file: url }} />,
        );
        fireEvent.click(screen.getByRole('button', { name: '下载' }));
        await screen.findByText('已下载');
        expect(chatGroupFilesService.download).toHaveBeenCalledWith(
            target,
            { kind: 'url', url },
            'report_final.pdf',
            expect.any(String),
            expect.any(Function),
        );
    });
});

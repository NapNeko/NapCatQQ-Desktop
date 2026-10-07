import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { GroupFileListing } from '../../../core/ipc/generated/chat/GroupFileListing';
import { chatGroupFilesService } from '../../../core/services/chat-group-files.service';
import { GroupFilesDialog } from './GroupFilesDialog';

const target: DebugTarget = {
    bot_id: 'files-test',
    name: '测试',
    qq_id: 10001,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};

function renderDialog(groupId = '20001', connected = true) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const content = (nextGroupId: string, nextTarget = target, open = true) => (
        <QueryClientProvider client={client}>
            <GroupFilesDialog
                open={open}
                onOpenChange={() => {}}
                target={nextTarget}
                groupId={nextGroupId}
                groupName="NapCat 开发交流"
                connected={connected}
                refreshSignal=""
            />
        </QueryClientProvider>
    );
    const view = render(content(groupId));
    return {
        ...view,
        update: (nextGroupId: string, nextTarget = target, open = true) =>
            view.rerender(content(nextGroupId, nextTarget, open)),
    };
}

describe('GroupFilesDialog', () => {
    afterEach(() => vi.restoreAllMocks());

    it('lists folders before files and opens a folder', async () => {
        renderDialog();
        const list = await screen.findByRole('list', { name: '群文件' });
        await within(list).findByText('NapCat 部署说明.pdf');
        const names = within(list)
            .getAllByRole('listitem')
            .map((item) => item.querySelector('.native-group-file-name')?.textContent);
        expect(names.slice(0, 2)).toEqual(['版本发布', '文档']);
        expect(within(list).getAllByText(/天后过期/).length).toBeGreaterThan(0);
        await userEvent.click(screen.getByRole('button', { name: '打开文件夹 文档' }));
        await screen.findByText('接口变更记录.md');
        expect(screen.getByRole('navigation', { name: '当前位置' }).textContent).toContain('文档');
        await userEvent.click(screen.getByRole('button', { name: '全部文件' }));
        await screen.findByText('NapCat 部署说明.pdf');
    });

    it('filters the current listing', async () => {
        renderDialog();
        await screen.findByText('config.example.json');
        await userEvent.type(screen.getByRole('textbox', { name: '搜索群文件' }), 'config');
        expect(screen.queryByText('截图合集.zip')).toBeNull();
        expect(screen.getByText('config.example.json')).toBeTruthy();
    });

    it('resets the folder and search when switching groups, accounts or reopening', async () => {
        const list = vi.spyOn(chatGroupFilesService, 'list');
        const view = renderDialog();
        await screen.findByRole('button', { name: '打开文件夹 文档' });
        await userEvent.click(screen.getByRole('button', { name: '打开文件夹 文档' }));
        await screen.findByText('接口变更记录.md');
        await userEvent.type(screen.getByRole('textbox', { name: '搜索群文件' }), '接口');

        view.update('20003');
        await screen.findByText('行程表.xlsx');
        expect(screen.getByRole('textbox', { name: '搜索群文件' })).toHaveValue('');
        expect(screen.getByRole('navigation', { name: '当前位置' }).textContent).toBe('全部文件');
        expect(list).toHaveBeenCalledWith(target, '20003', null, undefined);

        view.update('20001');
        await screen.findByRole('button', { name: '打开文件夹 文档' });
        await userEvent.click(screen.getByRole('button', { name: '打开文件夹 文档' }));
        await screen.findByText('接口变更记录.md');
        const otherTarget = { ...target, qq_id: 10002 };
        view.update('20001', otherTarget);
        await screen.findByText('config.example.json');
        expect(list).toHaveBeenCalledWith(otherTarget, '20001', null, undefined);

        await userEvent.click(screen.getByRole('button', { name: '打开文件夹 文档' }));
        await screen.findByText('接口变更记录.md');
        view.update('20001', otherTarget, false);
        view.update('20001', otherTarget, true);
        await screen.findByText('config.example.json');
        expect(screen.getByRole('navigation', { name: '当前位置' }).textContent).toBe('全部文件');
    });

    it('does not display a late folder response in another group', async () => {
        const originalList = chatGroupFilesService.list;
        const oldListing = await originalList(target, '20001', '/mock-docs');
        let resolveFolder!: (listing: GroupFileListing) => void;
        vi.spyOn(chatGroupFilesService, 'list').mockImplementation(
            (account, groupId, folderId, limit) =>
                folderId === '/mock-docs'
                    ? new Promise((resolve) => {
                          resolveFolder = resolve;
                      })
                    : originalList(account, groupId, folderId, limit),
        );
        const view = renderDialog();
        await screen.findByRole('button', { name: '打开文件夹 文档' });
        await userEvent.click(screen.getByRole('button', { name: '打开文件夹 文档' }));
        view.update('20003');
        await screen.findByText('行程表.xlsx');
        await act(async () => resolveFolder(oldListing));
        expect(screen.queryByText('接口变更记录.md')).toBeNull();
        expect(screen.getByText('行程表.xlsx')).toBeTruthy();
    });

    it('ignores files picked after leaving the original group', async () => {
        let resolvePick!: (files: { path: string; name: string }[]) => void;
        vi.spyOn(chatGroupFilesService, 'pickUploads').mockImplementation(
            () =>
                new Promise((resolve) => {
                    resolvePick = resolve;
                }),
        );
        const upload = vi.spyOn(chatGroupFilesService, 'upload');
        const view = renderDialog();
        await userEvent.click(screen.getByRole('button', { name: '上传' }));
        view.update('20003');
        await act(async () => resolvePick([{ path: 'preview://old.txt', name: 'old.txt' }]));
        expect(upload).not.toHaveBeenCalled();
    });

    it('creates a folder and deletes a file after confirming', async () => {
        renderDialog('20002');
        await screen.findByText('行程表.xlsx');
        await userEvent.click(screen.getByRole('button', { name: '新建文件夹' }));
        await userEvent.type(await screen.findByRole('textbox', { name: '文件夹名称' }), '照片');
        await userEvent.click(screen.getByRole('button', { name: '确定' }));
        await screen.findByRole('button', { name: '打开文件夹 照片' });
        await userEvent.click(screen.getByRole('button', { name: '行程表.xlsx的更多操作' }));
        await userEvent.click(await screen.findByRole('button', { name: '删除' }));
        const confirm = await screen.findByRole('dialog', { name: /删除「行程表.xlsx」/ });
        await userEvent.click(within(confirm).getByRole('button', { name: '删除' }));
        await waitFor(() => expect(screen.queryByText('行程表.xlsx')).toBeNull());
    });

    it('downloads with inline progress and offers to open the result', async () => {
        renderDialog();
        await screen.findByText('config.example.json');
        await userEvent.click(screen.getByRole('button', { name: '下载 config.example.json' }));
        await screen.findByRole('button', { name: '打开' }, { timeout: 4000 });
        expect(screen.getByText('已下载')).toBeTruthy();
    });

    it('explains why nothing loads while disconnected', () => {
        renderDialog('20001', false);
        expect(screen.getByText('连接断开，重新连接后可以浏览群文件')).toBeTruthy();
        expect((screen.getByRole('button', { name: '上传' }) as HTMLButtonElement).disabled).toBe(
            true,
        );
    });
});

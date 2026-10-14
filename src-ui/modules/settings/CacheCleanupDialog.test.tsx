import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { CacheCleanupResult, CacheScan } from '../../core/ipc/types';
import { CacheCleanupDialog } from './CacheCleanupDialog';

const service = vi.hoisted(() => ({ scan: vi.fn(), clean: vi.fn() }));
vi.mock('../../core/services/cache-cleanup.service', () => ({ cacheCleanupService: service }));

const snapshot: CacheScan = {
    busy: false,
    categories: [
        { category: 'installers', files: 2, bytes: BigInt(1024), skipped: 0, warnings: [] },
        { category: 'desktop_updates', files: 1, bytes: BigInt(2048), skipped: 1, warnings: [] },
        { category: 'download_residues', files: 3, bytes: BigInt(4096), skipped: 0, warnings: [] },
    ],
};
const result: CacheCleanupResult = {
    deleted_files: 3,
    deleted_bytes: BigInt(3000),
    skipped_files: 1,
    failed_files: 1,
    warnings: ['package.zip：权限不足'],
};

afterEach(cleanup);
beforeEach(() => {
    service.scan.mockReset().mockResolvedValue(snapshot);
    service.clean.mockReset().mockResolvedValue(result);
});

function show() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    const close = vi.fn();
    render(
        <QueryClientProvider client={client}>
            <CacheCleanupDialog open onOpenChange={close} />
        </QueryClientProvider>,
    );
    return close;
}

describe('下载缓存清理', () => {
    it('只提交勾选且有可清理文件的类别，显示部分失败并重新扫描', async () => {
        show();
        await screen.findByRole('checkbox', { name: /^安装包/ });
        fireEvent.click(screen.getByRole('checkbox', { name: /^安装包/ }));
        const clean = screen.getByRole('button', { name: '清理所选缓存' });
        await waitFor(() => expect(clean).toBeEnabled());
        fireEvent.click(clean);
        await waitFor(() =>
            expect(service.clean).toHaveBeenCalledWith(['desktop_updates', 'download_residues']),
        );
        await screen.findByText(/已删除 3 个文件/);
        expect(screen.getByText(/跳过 1 项，失败 1 项/)).toBeInTheDocument();
        expect(service.scan).toHaveBeenCalledTimes(2);
        expect(screen.getByText('package.zip：权限不足')).toBeInTheDocument();
    });

    it('活跃安装使清理不可用，重新扫描空缓存后也不能提交', async () => {
        service.scan.mockResolvedValueOnce({ ...snapshot, busy: true });
        show();
        await screen.findByText('安装或维护正在进行，完成后请重新扫描。');
        expect(screen.getByRole('button', { name: '清理所选缓存' })).toBeDisabled();
        service.scan.mockResolvedValue({
            ...snapshot,
            categories: snapshot.categories.map((group) => ({
                ...group,
                files: 0,
                bytes: BigInt(0),
            })),
        });
        fireEvent.click(screen.getByRole('button', { name: '重新扫描' }));
        await waitFor(() =>
            expect(
                screen.queryByText('安装或维护正在进行，完成后请重新扫描。'),
            ).not.toBeInTheDocument(),
        );
        expect(screen.getByRole('button', { name: '清理所选缓存' })).toBeDisabled();
        expect(service.clean).not.toHaveBeenCalled();
    });

    it('清理期间防止重复提交和关闭', async () => {
        let finish!: (value: CacheCleanupResult) => void;
        service.clean.mockImplementation(
            () =>
                new Promise<CacheCleanupResult>((resolve) => {
                    finish = resolve;
                }),
        );
        const close = show();
        const clean = await screen.findByRole('button', { name: '清理所选缓存' });
        await waitFor(() => expect(clean).toBeEnabled());
        fireEvent.click(clean);
        fireEvent.click(clean);
        expect(service.clean).toHaveBeenCalledTimes(1);
        expect(screen.getByRole('button', { name: '关闭' })).toBeDisabled();
        expect(close).not.toHaveBeenCalled();
        finish(result);
        await screen.findByText(/已删除 3 个文件/);
    });

    it('扫描失败时禁用清理，重试后恢复', async () => {
        service.scan.mockRejectedValueOnce('目录不可读');
        show();
        expect(await screen.findByRole('alert')).toHaveTextContent('目录不可读');
        expect(screen.getByRole('button', { name: '清理所选缓存' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '重新扫描' }));
        await waitFor(() =>
            expect(screen.getByRole('button', { name: '清理所选缓存' })).toBeEnabled(),
        );
    });

    it('清理的并发拒绝显示原因，保留弹窗供重试', async () => {
        service.clean.mockRejectedValueOnce(new Error('安装正在进行'));
        const close = show();
        const clean = await screen.findByRole('button', { name: '清理所选缓存' });
        await waitFor(() => expect(clean).toBeEnabled());
        fireEvent.click(clean);
        await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('安装正在进行'));
        expect(close).not.toHaveBeenCalled();
        await waitFor(() => expect(service.scan).toHaveBeenCalledTimes(2));
    });
});

import { beforeEach, describe, expect, it, vi } from 'vitest';

const transport = vi.hoisted(() => ({ native: true, invoke: vi.fn() }));

vi.mock('../ipc/transport', () => ({
    get isTauri() {
        return transport.native;
    },
    invoke: transport.invoke,
}));

let service: typeof import('./bot-migration.service').botMigrationService;
const request = { bot_id: '10001', target_server_id: 'server-b' };

beforeEach(async () => {
    transport.native = true;
    transport.invoke.mockReset();
    vi.resetModules();
    service = (await import('./bot-migration.service')).botMigrationService;
});

describe('botMigrationService', () => {
    it('预检和开始命令保留 request 的领域字段与预检指纹', async () => {
        const plan = { fingerprint: 'source-and-target-state' };
        const job = { id: 'migration-1', phase: 'preparing' };
        transport.invoke.mockResolvedValueOnce(plan).mockResolvedValueOnce(job);

        await expect(service.preview(request)).resolves.toBe(plan);
        await expect(service.start(request, plan.fingerprint)).resolves.toBe(job);

        expect(transport.invoke.mock.calls).toEqual([
            [
                'preview_bot_migration',
                { request: { bot_id: '10001', target_server_id: 'server-b' } },
            ],
            ['start_bot_migration', { request, fingerprint: plan.fingerprint }],
        ]);
    });

    it('恢复记录和回退命令使用 Tauri camelCase 参数', async () => {
        transport.invoke.mockResolvedValue(null);

        await expect(service.get('10001')).resolves.toBeNull();
        await service.rollback('migration-1', true);
        await service.rollback('migration-2', false);

        expect(transport.invoke.mock.calls).toEqual([
            ['get_bot_migration', { botId: '10001' }],
            ['rollback_bot_migration', { jobId: 'migration-1', acceptDataLoss: true }],
            ['rollback_bot_migration', { jobId: 'migration-2', acceptDataLoss: false }],
        ]);
    });

    it('浏览器预览不能执行迁移，也不会伪造任务成功', async () => {
        transport.native = false;
        vi.resetModules();
        const browserService = (await import('./bot-migration.service')).botMigrationService;

        expect(browserService.available).toBe(false);
        const unavailable = '浏览器预览无法执行服务器迁移，请使用桌面客户端。';
        await expect(browserService.preview(request)).rejects.toThrow(unavailable);
        await expect(browserService.start(request, 'fingerprint')).rejects.toThrow(unavailable);
        await expect(browserService.get('10001')).rejects.toThrow(unavailable);
        await expect(browserService.rollback('migration-1', true)).rejects.toThrow(unavailable);
        expect(transport.invoke).not.toHaveBeenCalled();
    });

    it('后端失败保留原错误供弹窗显示', async () => {
        const error = new Error('目标服务器磁盘空间不足');
        transport.invoke.mockRejectedValue(error);

        await expect(service.preview(request)).rejects.toBe(error);
        await expect(service.start(request, 'fingerprint')).rejects.toBe(error);
        await expect(service.get('10001')).rejects.toBe(error);
        await expect(service.rollback('migration-1', true)).rejects.toBe(error);
    });
});

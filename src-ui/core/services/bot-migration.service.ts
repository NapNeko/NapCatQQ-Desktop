// 整套 Bot 迁移的 IPC 入口。

import { invoke, isTauri } from '../ipc/transport';
import type { BotMigrationRequest } from '../ipc/generated/domain/BotMigrationRequest';
import type { BotMigrationPlan } from '../ipc/generated/domain/BotMigrationPlan';
import type { BotMigrationJob } from '../ipc/generated/domain/BotMigrationJob';

function requireDesktop() {
    if (!isTauri) throw new Error('浏览器预览无法执行服务器迁移，请使用桌面客户端。');
}

export const botMigrationService = {
    available: isTauri,

    preview: async (request: BotMigrationRequest): Promise<BotMigrationPlan> => {
        requireDesktop();
        return invoke<BotMigrationPlan>('preview_bot_migration', { request });
    },

    start: async (request: BotMigrationRequest, fingerprint: string): Promise<BotMigrationJob> => {
        requireDesktop();
        return invoke<BotMigrationJob>('start_bot_migration', { request, fingerprint });
    },

    get: async (botId: string): Promise<BotMigrationJob | null> => {
        requireDesktop();
        return invoke<BotMigrationJob | null>('get_bot_migration', { botId });
    },

    rollback: async (jobId: string, acceptDataLoss: boolean): Promise<BotMigrationJob> => {
        requireDesktop();
        return invoke<BotMigrationJob>('rollback_bot_migration', { jobId, acceptDataLoss });
    },
};

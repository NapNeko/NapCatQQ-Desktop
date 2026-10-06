// Koishi 专有接口：运行状态、插件表单来源、已装插件包、重启 worker。插件树的读写走通用的应用端配置接口。

import { invoke, isTauri } from '../ipc/transport';
import { mockAppFrameworkApi } from '../ipc/mock/app-framework.mock';
import type {
    KoishiCommandRow,
    KoishiDatabaseTable,
    KoishiFileContent,
    KoishiFileEntry,
    KoishiPackageInfo,
    KoishiPluginSchema,
    KoishiRuntimeStatus,
    KoishiSandboxMessage,
} from '../ipc/types';

export const koishiService = {
    status: async (instanceId: string): Promise<KoishiRuntimeStatus> => {
        if (!isTauri) return mockAppFrameworkApi.koishiStatus(instanceId);
        return invoke<KoishiRuntimeStatus>('koishi_status', { instanceId });
    },

    /** 空串 = 全局设置 */
    pluginSchemas: async (instanceId: string, names: string[]): Promise<KoishiPluginSchema[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiPluginSchemas(instanceId, names);
        return invoke<KoishiPluginSchema[]>('koishi_plugin_schemas', { instanceId, names });
    },

    packages: async (instanceId: string): Promise<KoishiPackageInfo[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiPackages(instanceId);
        return invoke<KoishiPackageInfo[]>('koishi_packages', { instanceId });
    },

    restart: async (instanceId: string): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiRestart(instanceId);
        return invoke<void>('koishi_restart', { instanceId });
    },

    // ---- 控制台功能（沙盒试聊 / 文件 / 数据库 / 指令），实例要在跑 ----

    sandboxSend: async (
        instanceId: string,
        msg: { platform: string; user: string; channel: string; content: string },
    ): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiSandboxSend(instanceId, msg);
        return invoke<void>('koishi_sandbox_send', { instanceId, ...msg });
    },

    sandboxMessages: async (instanceId: string): Promise<KoishiSandboxMessage[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiSandboxMessages(instanceId);
        return invoke<KoishiSandboxMessage[]>('koishi_sandbox_messages', { instanceId });
    },

    explorerTree: async (instanceId: string): Promise<KoishiFileEntry[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiExplorerTree(instanceId);
        return invoke<KoishiFileEntry[]>('koishi_explorer_tree', { instanceId });
    },

    explorerRead: async (instanceId: string, path: string): Promise<KoishiFileContent> => {
        if (!isTauri) return mockAppFrameworkApi.koishiExplorerRead(instanceId, path);
        return invoke<KoishiFileContent>('koishi_explorer_read', { instanceId, path });
    },

    explorerWrite: async (
        instanceId: string,
        path: string,
        content: string,
        binary = false,
    ): Promise<void> => {
        if (!isTauri)
            return mockAppFrameworkApi.koishiExplorerWrite(instanceId, path, content, binary);
        return invoke<void>('koishi_explorer_write', { instanceId, path, content, binary });
    },

    explorerMkdir: async (instanceId: string, path: string): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiExplorerMkdir(instanceId, path);
        return invoke<void>('koishi_explorer_mkdir', { instanceId, path });
    },

    explorerRemove: async (instanceId: string, path: string): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiExplorerRemove(instanceId, path);
        return invoke<void>('koishi_explorer_remove', { instanceId, path });
    },

    explorerRename: async (instanceId: string, from: string, to: string): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiExplorerRename(instanceId, from, to);
        return invoke<void>('koishi_explorer_rename', { instanceId, from, to });
    },

    databaseTables: async (instanceId: string): Promise<KoishiDatabaseTable[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiDatabaseTables(instanceId);
        return invoke<KoishiDatabaseTable[]>('koishi_database_tables', { instanceId });
    },

    databaseRows: async (
        instanceId: string,
        table: string,
        offset: number,
        limit: number,
    ): Promise<Record<string, unknown>[]> => {
        if (!isTauri)
            return mockAppFrameworkApi.koishiDatabaseRows(instanceId, table, offset, limit);
        return invoke<Record<string, unknown>[]>('koishi_database_rows', {
            instanceId,
            table,
            offset,
            limit,
        });
    },

    commands: async (instanceId: string): Promise<KoishiCommandRow[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiCommands(instanceId);
        return invoke<KoishiCommandRow[]>('koishi_commands', { instanceId });
    },

    commandUpdate: async (
        instanceId: string,
        name: string,
        config: Record<string, unknown>,
    ): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiCommandUpdate(instanceId, name, config);
        return invoke<void>('koishi_command_update', { instanceId, name, config });
    },

    commandAliases: async (instanceId: string, name: string, aliases: string[]): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiCommandAliases(instanceId, name, aliases);
        return invoke<void>('koishi_command_aliases', { instanceId, name, aliases });
    },
};

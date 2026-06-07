// ServerManager IPC 服务层。
// 对接后端 commands/servers.rs 的 5 个 CRUD/test Tauri 命令。
// 部署走组件页（host_id = "remote:<id>"），不在这层。

import { invoke, isTauri } from '../ipc/transport';
import type { ServerProfile } from '../ipc/generated/domain/ServerProfile';
import type { ProbeReport } from '../ipc/generated/domain/ProbeReport';

export const serverService = {
    list: async (): Promise<ServerProfile[]> => {
        if (isTauri) return invoke<ServerProfile[]>('list_servers');
        return [];
    },

    add: async (profile: ServerProfile, password?: string): Promise<ServerProfile> => {
        if (isTauri) return invoke<ServerProfile>('add_server', { profile, password: password ?? null });
        throw new Error('not in Tauri');
    },

    update: async (profile: ServerProfile, password?: string): Promise<ServerProfile> => {
        if (isTauri) return invoke<ServerProfile>('update_server', { profile, password: password ?? null });
        throw new Error('not in Tauri');
    },

    delete: async (id: string): Promise<void> => {
        if (isTauri) return invoke<void>('delete_server', { id });
    },

    testConnection: async (id: string, password?: string): Promise<ProbeReport> => {
        if (isTauri) return invoke<ProbeReport>('test_server_connection', { id, password: password ?? null });
        return {
            success: false,
            osInfo: null,
            error: 'not in Tauri',
            latencyMs: BigInt(0),
            hostKeyPrompt: null,
            hostKeyMismatch: false,
        };
    },

    /// 用户在指纹确认弹窗点"信任"后调用:把这把 host key 写进 known_hosts。
    /// 之后该主机的连接(测试 / 配免密 / 自动重连)即可通过 TOFU 校验。
    confirmHostKey: async (id: string, keyKind: string, keyB64: string): Promise<void> => {
        if (isTauri) return invoke<void>('confirm_server_host_key', { id, keyKind, keyB64 });
        throw new Error('not in Tauri');
    },

    /// 密码登录 → 自动配置免密：把本地新生成的公钥写进远端 authorized_keys，
    /// 档案切到密钥认证。成功返回更新后的档案。
    setupKeyAuth: async (id: string, password: string): Promise<ServerProfile> => {
        if (isTauri) return invoke<ServerProfile>('setup_server_key_auth', { id, password });
        throw new Error('not in Tauri');
    },

    /// 扫描 ~/.ssh/ 下标准命名私钥，返回路径列表（id_ed25519 / id_ecdsa / id_rsa / id_dsa）。
    scanLocalSshKeys: async (): Promise<string[]> => {
        if (isTauri) return invoke<string[]>('scan_local_ssh_keys');
        return [];
    },
};

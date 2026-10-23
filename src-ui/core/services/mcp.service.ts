import { invoke, isTauri } from '../ipc/transport';
import type { McpServerStatus } from '../ipc/generated/domain/McpServerStatus';

export const mcpService = {
    status: (): Promise<McpServerStatus> =>
        isTauri
            ? invoke('mcp_status')
            : Promise.resolve({ enabled: false, listening: false, port: null, lastError: null }),
    async copyConnectionConfig(): Promise<void> {
        if (!isTauri) throw new Error('请在桌面应用中复制连接配置');
        // 密钥只在用户点击后读取，不进入查询缓存、组件状态或日志。
        const config = await invoke<string>('mcp_connection_config');
        await navigator.clipboard.writeText(config);
    },
    rotateToken: (): Promise<void> =>
        isTauri ? invoke('mcp_rotate_token') : Promise.reject(new Error('请在桌面应用中重置令牌')),
};

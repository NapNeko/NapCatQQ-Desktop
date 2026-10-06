// 浏览器预览：麦麦 WebUI 的运行期接口（状态 / 用量 / 会话 / 提供商与 MCP 探测）。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import { MAIBOT_PLACEHOLDER_API_KEY } from '../../domain/apps/maibotConfig';
import type {
    AppInstance,
    MaiBotAPIProvider,
    MaiBotChatSession,
    MaiBotMCPServerItemConfig,
    MaiBotMcpStatus,
    MaiBotMcpTest,
    MaiBotProviderCheck,
    MaiBotProviderModel,
    MaiBotRuntimeStatus,
    MaiBotStatsSummary,
} from '../types';
import { peekMaiBotConfig } from './app-config.mock';
import { withMockDelay } from './bootstrap.mock';

// 进程启动时刻（毫秒）；从 WebUI 重启后重记，uptime 从 0 算
const startedAt = new Map<string, number>();

function requireRunning(inst: AppInstance) {
    if (inst.state !== 'running') {
        throw makeAppConfigError('not_running', '启动麦麦后才能用');
    }
    if (!startedAt.has(inst.id)) startedAt.set(inst.id, Date.now() - 42 * 60_000);
}

const MODELS_BY_HOST: [RegExp, string[]][] = [
    [/deepseek/, ['deepseek-chat', 'deepseek-reasoner', 'deepseek-v4-pro', 'deepseek-v4-flash']],
    [/siliconflow/, ['Qwen/Qwen3-235B-A22B', 'deepseek-ai/DeepSeek-V3', 'BAAI/bge-m3']],
    [/moonshot/, ['kimi-k2-0711-preview', 'moonshot-v1-8k']],
    [/127\.0\.0\.1|localhost/, ['qwen3:8b', 'llama3.1:8b']],
];

export const mockMaiBotRuntime = {
    status(inst: AppInstance): Promise<MaiBotRuntimeStatus> {
        if (inst.state !== 'running') {
            return withMockDelay({ gate: 'not_running', message: '启动麦麦后才能看运行状态' });
        }
        requireRunning(inst);
        const uptime = (Date.now() - (startedAt.get(inst.id) ?? Date.now())) / 1000;
        return withMockDelay({ gate: 'ok', message: '', version: '1.2.5', uptime_secs: uptime });
    },

    restart(inst: AppInstance): Promise<void> {
        requireRunning(inst);
        startedAt.set(inst.id, Date.now());
        return withMockDelay(undefined);
    },

    stats(inst: AppInstance, hours: number): Promise<MaiBotStatsSummary> {
        requireRunning(inst);
        const k = Math.max(1, Math.min(hours, 24)) / 24;
        return withMockDelay({
            total_messages: Math.round(386 * k),
            total_replies: Math.round(57 * k),
            total_requests: Math.round(212 * k),
            total_tokens: Math.round(418_230 * k),
            total_cost: Number((1.37 * k).toFixed(2)),
            avg_response_time: 3.8,
        });
    },

    chatSessions(inst: AppInstance): Promise<MaiBotChatSession[]> {
        requireRunning(inst);
        const now = Date.now() / 1000;
        return withMockDelay([
            {
                session_id: 's1',
                display_name: '麦麦测试群',
                chat_type: 'group',
                target_id: '123456789',
                platform: 'qq',
                message_count: 318,
                last_active_at: now - 120,
            },
            {
                session_id: 's2',
                display_name: '摸鱼小分队',
                chat_type: 'group',
                target_id: '987654321',
                platform: 'qq',
                message_count: 64,
                last_active_at: now - 7200,
            },
            {
                session_id: 's3',
                display_name: '小明',
                chat_type: 'private',
                target_id: '10001',
                platform: 'qq',
                message_count: 12,
                last_active_at: now - 86_400,
            },
        ]);
    },

    providerModels(inst: AppInstance, p: MaiBotAPIProvider): Promise<MaiBotProviderModel[]> {
        requireRunning(inst);
        if (
            p.auth_type !== 'none' &&
            (!p.api_key.trim() || p.api_key.trim() === MAIBOT_PLACEHOLDER_API_KEY)
        ) {
            throw makeAppConfigError('other', 'API Key 无效或已过期');
        }
        const ids = MODELS_BY_HOST.find(([re]) => re.test(p.base_url))?.[1] ?? [
            'gpt-4o-mini',
            'gpt-4.1',
        ];
        return withMockDelay(ids.map((id) => ({ id, name: id })));
    },

    testProvider(inst: AppInstance, p: MaiBotAPIProvider): Promise<MaiBotProviderCheck> {
        requireRunning(inst);
        if (!/^https?:\/\//.test(p.base_url.trim())) {
            return withMockDelay({ network_ok: false, error: '连接失败：无法连接到服务器' });
        }
        const key = p.api_key.trim();
        if (!key) return withMockDelay({ network_ok: true, latency_ms: 86.4, error: '' });
        const valid = key !== MAIBOT_PLACEHOLDER_API_KEY;
        return withMockDelay({
            network_ok: true,
            api_key_valid: valid,
            latency_ms: 91.2,
            error: valid ? '' : 'API Key 无效或已过期',
        });
    },

    mcpStatus(inst: AppInstance): Promise<MaiBotMcpStatus> {
        requireRunning(inst);
        const mcp = peekMaiBotConfig(inst).bot.mcp;
        const servers = mcp.enable ? mcp.servers.filter((s) => s.enabled) : [];
        return withMockDelay({
            initialized: true,
            servers: servers.map((s, i) => ({
                name: s.name,
                transport: s.transport,
                connected: i % 3 !== 2,
                tool_count: i % 3 !== 2 ? 3 : 0,
                error: i % 3 !== 2 ? '' : '连接被拒绝',
            })),
        });
    },

    testMcp(inst: AppInstance, server: MaiBotMCPServerItemConfig): Promise<MaiBotMcpTest> {
        requireRunning(inst);
        const target = server.transport === 'stdio' ? server.command : server.url;
        if (!target.trim()) {
            return withMockDelay({ success: false, error: '没有可连接的目标', tools: [] });
        }
        return withMockDelay({
            success: true,
            error: '',
            tools: [
                { name: 'read_file', title: '读文件', description: '读取允许目录下的文件内容' },
                { name: 'list_directory', title: '', description: '列出目录' },
            ],
        });
    },
};

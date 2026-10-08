// 麦麦的假数据与运行时能力转发：市场清单在本文件（git 安装源，官方件带 locked 标记），
// 运行时数据本体在 maibot-runtime.mock，这里补实例查找与重启后的日志回放。
import type {
    AppStoreMarketEntry,
    MaiBotAPIProvider,
    MaiBotChatSession,
    MaiBotMCPServerItemConfig,
    MaiBotMcpStatus,
    MaiBotMcpTest,
    MaiBotProviderCheck,
    MaiBotProviderModel,
    MaiBotRuntimeStatus,
    MaiBotStatsSummary,
} from '../../types';
import { mockMaiBotRuntime } from '../maibot-runtime.mock';
import { playMockAppRun } from '../app-log.mock';
import { require } from './state';

export const maibotMarketEntry = (
    id: string,
    name: string,
    description: string,
    author: string,
    repo: string,
): AppStoreMarketEntry => ({
    resource: 'plugin',
    id,
    name,
    description,
    version: '1.0.0',
    author,
    homepage: `https://github.com/${repo}`,
    time: '',
    package: `https://github.com/${repo}`,
    module_name: '',
    flavor: 'git',
    is_official: id.startsWith('maibot-team.'),
    valid: true,
    tags: [],
    supported_adapters: [],
    authors: [],
    repos: [],
    files: [],
    allow_build: [],
});

export const mockMaiBotPlugins: AppStoreMarketEntry[] = [
    maibotMarketEntry(
        'maibot-team.napcat-adapter',
        'Napcat_Adapter 适配器',
        '插件版 Napcat 适配器，提供与 Napcat 的连接功能。',
        'MaiBot Team',
        'Mai-with-u/MaiBot-Napcat-Adapter',
    ),
    maibotMarketEntry(
        'sengokucola.mute-plugin',
        '群聊禁言管理插件',
        '智能禁言和手动禁言命令',
        'SengokuCola',
        'SengokuCola/MutePlugin',
    ),
    maibotMarketEntry(
        'a0000xz.maibot-tarots-plugin',
        '塔罗牌插件',
        '抽一张塔罗牌，麦麦来解读',
        'A0000Xz',
        'A0000Xz/MaiBot-Tarots-Plugin',
    ),
];

export const mockMaiBotApi = {
    maibotStatus: (instanceId: string): Promise<MaiBotRuntimeStatus> =>
        mockMaiBotRuntime.status(require(instanceId)),
    maibotRestart: async (instanceId: string): Promise<void> => {
        await mockMaiBotRuntime.restart(require(instanceId));
        playMockAppRun(require(instanceId), () => require(instanceId).state === 'running');
    },
    maibotStats: (instanceId: string, hours: number): Promise<MaiBotStatsSummary> =>
        mockMaiBotRuntime.stats(require(instanceId), hours),
    maibotChatSessions: (instanceId: string): Promise<MaiBotChatSession[]> =>
        mockMaiBotRuntime.chatSessions(require(instanceId)),
    maibotProviderModels: (
        instanceId: string,
        provider: MaiBotAPIProvider,
    ): Promise<MaiBotProviderModel[]> =>
        mockMaiBotRuntime.providerModels(require(instanceId), provider),
    maibotTestProvider: (
        instanceId: string,
        provider: MaiBotAPIProvider,
    ): Promise<MaiBotProviderCheck> =>
        mockMaiBotRuntime.testProvider(require(instanceId), provider),
    maibotMcpStatus: (instanceId: string): Promise<MaiBotMcpStatus> =>
        mockMaiBotRuntime.mcpStatus(require(instanceId)),
    maibotTestMcp: (
        instanceId: string,
        server: MaiBotMCPServerItemConfig,
    ): Promise<MaiBotMcpTest> => mockMaiBotRuntime.testMcp(require(instanceId), server),
};

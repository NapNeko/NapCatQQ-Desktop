// 浏览器预览模式下的应用端（Karin 等）假数据。
// 真 IPC 实装在 core/services/app-framework.service.ts。

import type {
    AppConfigDocument,
    AppConfigWriteResult,
    AppFrameworkManifest,
    AppInstance,
    AppInstanceWebUi,
    AppPendingTerms,
    AppPluginAction,
    AppPluginConfigSchema,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
    AppProjectProbe,
    AppWebUiAccount,
    AstrBotAbconfInfo,
    AstrBotDashboardStatus,
    AstrBotKbCreate,
    AstrBotKnowledgeBase,
    AstrBotPersona,
    AstrBotSessionRule,
    MaiBotAPIProvider,
    MaiBotChatSession,
    MaiBotMCPServerItemConfig,
    MaiBotMcpStatus,
    MaiBotMcpTest,
    MaiBotProviderCheck,
    MaiBotProviderModel,
    MaiBotRuntimeStatus,
    MaiBotStatsSummary,
    CreateAppInstanceRequest,
    DeploymentTaskSnapshot,
    DomainEvent,
    ImportAppInstanceRequest,
    KarinPluginInstalled,
    KoishiPackageInfo,
    KoishiPluginSchema,
    KoishiRuntimeStatus,
    AppPanelResult,
    KarinPluginMarketEntry,
    LogSnapshot,
    OneBotLinkPlan,
    PackageVersions,
    ProgressEvent,
    ProgressKind,
} from '../types';
import { mockAstrBotDashboard } from './astrbot-dashboard.mock';
import { mockMaiBotRuntime } from './maibot-runtime.mock';
import { latestStable } from '../../domain/apps/appVersions';
import { karinDefaultConfig } from '../../domain/apps/karinConfig';
import { astrbotDefaultConfig } from '../../domain/apps/astrbotConfig';
import { nonebot2DefaultConfig } from '../../domain/apps/nonebot2Config';
import { yunzaiDefaultConfig } from '../../domain/apps/yunzaiConfig';
import { emitMockEvent } from './events.mock';
import { withMockDelay } from './bootstrap.mock';
import { mockAppLogTail, playMockAppRun } from './app-log.mock';
import {
    koishiMockCommandAliases,
    koishiMockCommandUpdate,
    koishiMockCommands,
    koishiMockDatabaseRows,
    koishiMockDatabaseTables,
    koishiMockExplorerMkdir,
    koishiMockExplorerRead,
    koishiMockExplorerRemove,
    koishiMockExplorerRename,
    koishiMockExplorerTree,
    koishiMockExplorerWrite,
    koishiMockPackages,
    koishiMockSandboxMessages,
    koishiMockSandboxSend,
    koishiMockSchemas,
    koishiMockStatus,
} from './koishi.mock';
import { appendTo, isLinkNode, koishiShortName, linkNode, newPlugin, replaceAt, walk } from '../../domain/apps/koishiConfig';
import type { KoishiInstanceConfig } from '../types';
import {
    createMockAppConfigApi,
    editKoishiConfig,
    peekKoishiConfig,
    peekKarinHttpAuthKey,
    syncKarinLinkToken,
    syncMaiBotLink,
    syncYunzaiLink,
} from './app-config.mock';

export const mockAppFrameworks: AppFrameworkManifest[] = [
    {
        id: 'karin',
        display_name: 'Karin',
        description: '基于 Node.js 的轻量 Bot 框架，自带 WebUI 与插件生态。',
        repo_url: 'https://github.com/KarinJS/Karin',
        docs_url: 'https://karin.fun',
        supported_placements: ['local_native', 'remote_native'],
        default_port: 7777,
        has_webui: true,
        link_modes: ['reverse_ws'],
        component_id: 'karin',
        runtime_component_ids: ['nodejs'],
        store_resources: ['plugin'],
        has_install_renderer: true,
        webui_auth: 'key',
        terms: [],
    },
    {
        id: 'nonebot2',
        display_name: 'NoneBot2',
        description: 'Python 异步应用端，插件生态丰富；无 WebUI',
        repo_url: 'https://github.com/nonebot/nonebot2',
        docs_url: 'https://nonebot.dev',
        supported_placements: ['local_native', 'remote_native'],
        default_port: 8080,
        has_webui: false,
        link_modes: ['reverse_ws'],
        component_id: 'nonebot2',
        runtime_component_ids: ['uv'],
        store_resources: ['adapter', 'plugin'],
        has_install_renderer: false,
        webui_auth: 'none',
        terms: [],
    },
    {
        id: 'astrbot',
        display_name: 'AstrBot',
        description: 'Python 应用端，自带 WebUI；Desktop 只对接 OneBot v11',
        repo_url: 'https://github.com/AstrBotDevs/AstrBot',
        docs_url: 'https://docs.astrbot.app',
        supported_placements: ['local_native', 'remote_native'],
        default_port: 6199,
        has_webui: true,
        link_modes: ['reverse_ws'],
        component_id: 'astrbot',
        runtime_component_ids: ['uv'],
        store_resources: ['plugin'],
        has_install_renderer: false,
        webui_auth: 'user_password',
        terms: [],
    },
    {
        id: 'maibot',
        display_name: 'MaiBot',
        description: '麦麦，大模型驱动的拟人聊天应用端，自带 WebUI',
        repo_url: 'https://github.com/Mai-with-u/MaiBot',
        docs_url: 'https://docs.mai-mai.org',
        supported_placements: ['local_native', 'remote_native'],
        default_port: 8001,
        has_webui: true,
        link_modes: ['forward_ws'],
        component_id: 'maibot',
        runtime_component_ids: ['uv'],
        store_resources: ['plugin'],
        has_install_renderer: false,
        webui_auth: 'key',
        terms: [
            {
                id: 'eula',
                title: 'MaiBot 最终用户许可协议',
                url: 'https://github.com/Mai-with-u/MaiBot/blob/main/EULA.md',
            },
            {
                id: 'privacy',
                title: 'MaiBot 用户隐私条款',
                url: 'https://github.com/Mai-with-u/MaiBot/blob/main/PRIVACY.md',
            },
        ],
    },
    {
        id: 'yunzai',
        display_name: 'TRSS-Yunzai',
        description: '云崽 Node.js 应用端，喵喵插件那一套生态',
        repo_url: 'https://github.com/TimeRainStarSky/Yunzai',
        docs_url: 'https://github.com/TimeRainStarSky/Yunzai/tree/docs',
        supported_placements: ['local_native', 'remote_native'],
        default_port: 2536,
        has_webui: false,
        link_modes: ['reverse_ws'],
        component_id: 'yunzai',
        runtime_component_ids: ['git', 'nodejs', 'redis'],
        store_resources: ['plugin'],
        has_install_renderer: true,
        webui_auth: 'none',
        terms: [],
    },
];

mockAppFrameworks.push({
    id: 'neobot',
    display_name: 'NeoBot',
    description: 'Python 应用端，主打有活人感的聊天，自带 WebUI',
    repo_url: 'https://github.com/SuperQuail/NeoBot',
    docs_url: 'https://github.com/SuperQuail/NeoBot/tree/main/docs',
    supported_placements: ['local_native', 'remote_native'],
    default_port: 8080,
    has_webui: true,
    link_modes: ['reverse_ws'],
    component_id: 'neobot',
    runtime_component_ids: ['uv'],
    store_resources: [],
    has_install_renderer: false,
    webui_auth: 'none',
    terms: [],
});

mockAppFrameworks.push({
    id: 'koishi',
    display_name: 'Koishi',
    description: '跨平台聊天机器人框架，插件市场有几千个插件，自带控制台',
    repo_url: 'https://github.com/koishijs/koishi',
    docs_url: 'https://koishi.chat/zh-CN/',
    supported_placements: ['local_native', 'remote_native'],
    default_port: 5140,
    has_webui: true,
    link_modes: ['reverse_ws'],
    component_id: 'koishi',
    runtime_component_ids: ['nodejs'],
    store_resources: ['plugin'],
    has_install_renderer: false,
    webui_auth: 'none',
    terms: [],
});

/// 假装实例目录里的条款：MaiBot 实例启动前要同意一次，同意过的记在这里
const mockAcceptedTerms = new Set<string>();
const MOCK_TERMS_TEXT: Record<string, string> = {
    eula: '# MaiBot最终用户许可协议\n\n**版本：V1.3**\n\n1. 本项目免费开源，禁止倒卖。\n2. 使用本项目产生的内容由使用者自行负责。\n',
    privacy: '### MaiBot用户隐私条款\n\n**版本：V1.2**\n\n- 聊天记录只存在你自己的机器上。\n- 默认上报匿名统计，可在配置里关掉。\n',
};

/// 账号密码类 WebUI 的假账号：新建时按请求种入，重置时换密码。
const mockWebUiAccounts = new Map<string, { username: string; password: string | null }>([
    ['ab12cd34', { username: 'astrbot', password: 'Mock2024astrbot' }],
]);

function mockAccountView(inst: AppInstance): AppWebUiAccount | null {
    const manifest = mockAppFrameworks.find((m) => m.id === inst.framework_id);
    if (manifest?.webui_auth !== 'user_password') return null;
    const acct = mockWebUiAccounts.get(inst.id) ?? { username: 'astrbot', password: null };
    return {
        username: acct.username,
        password: acct.password ?? undefined,
        password_matches: acct.password ? true : undefined,
        can_reset: inst.state !== 'running',
    };
}

function mockGeneratePassword(): string {
    const pool = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789';
    let out = 'Aa1';
    for (let i = 0; i < 21; i += 1) out += pool[Math.floor(Math.random() * pool.length)];
    return out;
}

let instances: AppInstance[] = [
    {
        id: 'k1a2b3c4',
        framework_id: 'karin',
        display_name: 'Karin · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/karin/k1a2b3c4',
        port: 7777,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:k1a2b3c4',
            linked_at_ms: Date.now() - 3_600_000,
        },
        installed_version: '1.17.0',
        created_at_ms: Date.now() - 86_400_000,
        install_renderer: true,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'd5e6f7a8',
        framework_id: 'karin',
        display_name: 'Karin · production',
        placement: 'remote_native',
        host_id: 'remote:production',
        install_dir: '/home/ubuntu/ncd/apps/karin/d5e6f7a8',
        port: 7777,
        state: 'not_installed',
        created_at_ms: Date.now() - 600_000,
        install_renderer: true,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'n9b8c7d6',
        framework_id: 'nonebot2',
        display_name: '荒境修仙',
        placement: 'remote_native',
        host_id: 'remote:production',
        install_dir: '/root/game-qqbot/bot-xiuxian',
        port: 13120,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:n9b8c7d6',
            linked_at_ms: Date.now() - 1_800_000,
        },
        installed_version: '2.5.1',
        created_at_ms: Date.now() - 7_200_000,
        install_renderer: false,
        origin: 'imported',
        auto_start: true,
    },
    {
        id: 'ab12cd34',
        framework_id: 'astrbot',
        display_name: 'AstrBot · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/astrbot/ab12cd34',
        port: 6199,
        state: 'running',
        created_at_ms: Date.now() - 3_600_000,
        install_renderer: false,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'mb56ef78',
        framework_id: 'maibot',
        display_name: '麦麦 · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/maibot/mb56ef78',
        port: 23001,
        state: 'stopped',
        installed_version: '1.2.5',
        created_at_ms: Date.now() - 1_200_000,
        install_renderer: false,
        origin: 'created',
        auto_start: false,
    },
    {
        id: 'ko90ab12',
        framework_id: 'koishi',
        display_name: 'Koishi · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/koishi/ko90ab12',
        port: 23140,
        state: 'running',
        installed_version: '4.18.11',
        created_at_ms: Date.now() - 600_000,
        install_renderer: false,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'nb7c1d20',
        framework_id: 'neobot',
        display_name: 'NeoBot · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/neobot/nb7c1d20',
        port: 8080,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:nb7c1d20',
            linked_at_ms: Date.now() - 1_200_000,
        },
        installed_version: '1.2.0',
        created_at_ms: Date.now() - 3_600_000,
        install_renderer: false,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'yz90ab12',
        framework_id: 'yunzai',
        display_name: '云崽 · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/yunzai/yz90ab12',
        port: 2536,
        state: 'stopped',
        installed_version: '3.1.3',
        created_at_ms: Date.now() - 900_000,
        install_renderer: true,
        origin: 'created',
        auto_start: false,
    },
];

/** 对接写 / 摘 koishi.yml 里的 adapter-onebot:ncd-link（真机由后端 apply_link / unlink 写） */
function syncKoishiLink(inst: AppInstance, botId: string | null) {
    editKoishiConfig(inst, (cfg: KoishiInstanceConfig) => {
        const existing = linkNode(cfg);
        if (!botId) {
            if (!existing) return cfg;
            const path = findPath(cfg, isLinkNode);
            return path ? replaceAt(cfg, path, (n) => ({ ...n, enabled: false })) : cfg;
        }
        const config = { selfId: botId, token: 'mockmockmockmockmockmock', protocol: 'ws-reverse', path: '/onebot/ncd' };
        const path = findPath(cfg, isLinkNode);
        if (path) return replaceAt(cfg, path, (n) => ({ ...n, enabled: true, config: { ...n.config, ...config } }));
        const group = cfg.plugins.findIndex((n) => n.name === 'group' && n.ident === 'adapter' && n.enabled);
        const node = { ...newPlugin(cfg, 'adapter-onebot', true), ident: 'ncd-link', config };
        return appendTo(cfg, group >= 0 ? [group] : [], node);
    });
}

function findPath(cfg: KoishiInstanceConfig, pred: (n: KoishiInstanceConfig['plugins'][number]) => boolean): number[] | null {
    const go = (list: KoishiInstanceConfig['plugins'], base: number[]): number[] | null => {
        for (let i = 0; i < list.length; i += 1) {
            if (pred(list[i])) return [...base, i];
            const hit = go(list[i].children, [...base, i]);
            if (hit) return hit;
        }
        return null;
    };
    return go(cfg.plugins, []);
}

function publish(instance: AppInstance, reason: string) {
    instances = instances.map((i) => (i.id === instance.id ? instance : i));
    emitMockEvent({ kind: 'app_instance_changed', instance, reason });
}

function require(id: string): AppInstance {
    const found = instances.find((i) => i.id === id);
    if (!found) throw new Error(`应用实例不存在: ${id}`);
    return found;
}

/** 别的 mock（麦麦资源页）按 id 取实例看它在不在跑 */
export function peekMockAppInstance(id: string): AppInstance {
    return require(id);
}

const MOCK_INSTALL_STEPS = ['解析 uv', '下载源码', '放置源码', '同步 Python 依赖', '预置端口与协议确认'];

// NeoBot 的发行清单，最新在前；listVersions 与「装到最新正式版」的回填共用同一份
const MOCK_NEOBOT_VERSIONS = ['1.2.1', '1.2.0', '1.1.0', '1.0.0', '1.0.0a25'];

/**
 * 按真机的事件顺序假装跑一次安装：任务快照、步骤进度、最后实例变成已安装。
 * 走查卡片 / 详情页的「安装中」进度用。
 */
function simulateInstallTask(inst: AppInstance, version: string | null = null): string {
    const taskId = `mock-install-${inst.id}-${Date.now()}`;
    const target = `${inst.framework_id}@${inst.id}`;
    const submittedAtMs = BigInt(Date.now());
    let events: ProgressEvent[] = [];
    const snapshot = (status: DeploymentTaskSnapshot['status']): DeploymentTaskSnapshot => ({
        taskId,
        kind: { kind: 'component_action', component_id: inst.framework_id, action: 'ensure_installed' },
        status,
        hostId: inst.host_id,
        title: `${target} ensure_installed`,
        resources: [{ kind: 'install_target', host_id: inst.host_id, target }],
        progressEvents: events,
        submittedAtMs,
        cancellable: true,
    });
    const push = (kind: ProgressKind) => {
        const event = { v: 1, timestamp_ms: BigInt(Date.now()), ...kind } as ProgressEvent;
        events = [...events, event];
        emitMockEvent({ kind: 'component_action_progress', task_id: taskId, event } as DomainEvent);
        emitMockEvent({ kind: 'deployment_task_changed', task: snapshot('running') } as DomainEvent);
    };

    emitMockEvent({ kind: 'deployment_task_changed', task: snapshot('queued') } as DomainEvent);
    const plan: Array<() => void> = [() => push({ kind: 'started', total_steps: MOCK_INSTALL_STEPS.length })];
    MOCK_INSTALL_STEPS.forEach((message, idx) => {
        const step = idx + 1;
        plan.push(() => push({ kind: 'step_begin', step, message }));
        plan.push(() => push({ kind: 'step_end', step, ok: true }));
    });
    plan.push(() => push({ kind: 'finished', ok: true }));
    plan.push(() => {
        emitMockEvent({ kind: 'deployment_task_changed', task: snapshot('success') } as DomainEvent);
        // 指定了版本就回填它——真实链路由 detect 从发行元数据回读，预览里照同样的口径表现。
        // NeoBot 不指定版本 = 装到最新正式版再回读，回填清单里的 latest 正式版，不留旧号
        const settled = require(inst.id);
        publish(
            {
                ...settled,
                state: 'installed',
                installed_version:
                    version ??
                    (inst.framework_id === 'neobot' ? latestStable(MOCK_NEOBOT_VERSIONS) : null) ??
                    settled.installed_version ??
                    '1.2.5',
            },
            'installed',
        );
    });
    plan.forEach((run, i) => setTimeout(run, 400 + i * 600));
    return taskId;
}

/** 预览里「记住的面板密码」：只记有没有，不存明文 */
const panelPasswords = new Set<string>();

export const mockAppFrameworkApi = {
    /**
     * 预览里只认概览页要的那一个端点，够把页面跑起来；其余路径如实报「未模拟」，
     * 免得预览里看着像通了、真机却是空的。
     */
    /**
     * 预览里为每个页签各备一份与真机同形的回包；没备的路径如实报「未模拟」，
     * 免得预览里看着像通了、真机却是空的。
     */
    panelCall: async (
        _id: string,
        method: string,
        path: string,
        _body?: unknown,
    ): Promise<AppPanelResult | null> => {
        const ok = (data: unknown) => withMockDelay({ kind: 'ok' as const, data });
        if (method === 'POST') {
            // 插件的几个动作：预览里只要「面板收下了」就够，列表不变也没关系
            const pluginAction =
                path === '/api/plugins/install' ||
                /^\/api\/plugins\/[^/]+\/(toggle|reload|uninstall)$/.test(path);
            if (pluginAction) {
                return withMockDelay({
                    kind: 'ok' as const,
                    data: { ok: true, message: '预览：已提交' },
                });
            }
            return withMockDelay({
                kind: 'failed' as const,
                message: '预览未模拟 ' + method + ' ' + path,
            });
        }
        if (method !== 'GET') {
            return withMockDelay({
                kind: 'failed' as const,
                message: '预览未模拟 ' + method + ' ' + path,
            });
        }
        switch (path) {
            case '/api/overview':
                return ok({
                    ok: true,
                    online: true,
                    app_name: 'NeoBot',
                    app_version: '1.2.3',
                    bot_nickname: 'Luna',
                    bot_user_id: 10001,
                    avatar_url: '',
                    uptime_seconds: 8130,
                    today_messages: 128,
                    total_messages: 20461,
                    plugins_loaded: 6,
                    plugins_total: 7,
                    plugins_error: 1,
                    latency_ms: 42,
                    python_version: '3.13.5',
                    hostname: 'DESKTOP-LUNA',
                    standby: false,
                    notices: [
                        {
                            level: 'warning',
                            text: '尚未配置超级管理员账号，可能影响部分命令使用',
                            hint: '在「配置管理 → 本体配置 → chat」里填 admin_accounts（QQ 号列表）',
                        },
                    ],
                });
            case '/api/plugins':
                return ok({
                    ok: true,
                    manage_enabled: true,
                    console_plugin: 'dashboard',
                    items: [
                        {
                            id: 'dashboard',
                            name: 'dashboard',
                            version: '1.2.3',
                            status: '运行中',
                            state: 'running',
                            enabled: true,
                            description: '网页面板本体',
                            official: true,
                            manageable: false,
                            tags: ['official', 'web'],
                            python_dependencies: [],
                            missing_python_dependencies: [],
                            config_path: '',
                        },
                        {
                            id: 'demo-plugin',
                            name: 'demo-plugin',
                            version: '0.3.1',
                            status: '已加载',
                            state: 'loaded',
                            enabled: true,
                            description: '示例第三方插件',
                            author: 'someone',
                            official: false,
                            manageable: true,
                            repo: 'https://github.com/someone/demo-plugin',
                            tags: ['demo'],
                            python_dependencies: ['httpx'],
                            missing_python_dependencies: ['httpx'],
                            config_path: 'D:/ncd/apps/neobot/plugins_data/demo-plugin/config.toml',
                        },
                    ],
                });
            case '/api/config/models':
                return ok({
                    ok: true,
                    can_manage: true,
                    platforms: { DeepSeek: { name: 'DeepSeek', url: 'https://api.deepseek.com', has_key: true } },
                    assignments: { chat: 'deepseek-chat', vision: 'deepseek-vl' },
                    library: [
                        {
                            model_ref: 'deepseek-chat',
                            display_name: 'DeepSeek 对话',
                            provider: 'DeepSeek',
                            model_name: 'deepseek-chat',
                            model_type: 'chat',
                            type_label: '对话',
                        },
                        {
                            model_ref: 'deepseek-vl',
                            display_name: 'DeepSeek 视觉',
                            provider: 'DeepSeek',
                            model_name: 'deepseek-vl',
                            model_type: 'vision',
                            type_label: '视觉',
                        },
                        {
                            model_ref: 'silicon-chat',
                            display_name: 'SiliconFlow 对话',
                            provider: 'SiliconFlow',
                            model_name: 'Qwen/Qwen2.5-7B-Instruct',
                            model_type: 'chat',
                            type_label: '对话',
                        },
                    ],
                });
            case '/api/prompts':
                return ok({
                    ok: true,
                    editable: true,
                    sections: [
                        {
                            name: 'reply',
                            keys: [
                                {
                                    path: 'system',
                                    label: '系统提示词',
                                    kind: 'template',
                                    value: '你是 {nickname}，一个群里的普通成员。',
                                    default: '你是 {nickname}。',
                                    custom: '你是 {nickname}，一个群里的普通成员。',
                                    overridden: true,
                                    placeholders: ['nickname'],
                                },
                            ],
                        },
                        {
                            name: 'memory',
                            keys: [
                                {
                                    path: 'summary_hint',
                                    label: '摘要提示词',
                                    kind: 'scalar',
                                    value: '把要点压到 200 字以内。',
                                    default: '',
                                    custom: null,
                                    overridden: false,
                                    placeholders: [],
                                },
                            ],
                        },
                    ],
                });
            case '/api/archives':
                return ok({
                    ok: true,
                    can_manage: true,
                    summarize_available: true,
                    min_target_chars: 200,
                    max_target_chars: 2000,
                    items: [
                        { table_name: 'group_10001', count: 412, over_limit: 0, max_chars: 2000 },
                        { table_name: 'private_20002', count: 88, over_limit: 3, max_chars: 2000 },
                    ],
                });
            default:
                return withMockDelay({
                    kind: 'failed' as const,
                    message: '预览未模拟 ' + method + ' ' + path,
                });
        }
    },

    setPanelPassword: async (id: string, password: string): Promise<void> => {
        if (password.trim()) panelPasswords.add(id);
        else panelPasswords.delete(id);
    },

    panelPasswordSet: async (id: string): Promise<boolean> => withMockDelay(panelPasswords.has(id)),

    listFrameworks: () => withMockDelay(mockAppFrameworks),
    listInstances: () => withMockDelay(instances.slice()),

    previewInstallDir: async (hostId: string, frameworkId: string): Promise<string> =>
        withMockDelay(
            hostId === 'local'
                ? `D:/NapCatQQ/apps/${frameworkId}`
                : `/home/ubuntu/ncd/apps/${frameworkId}`,
        ),

    create: async (req: CreateAppInstanceRequest): Promise<AppInstance> => {
        const id = Math.random().toString(16).slice(2, 10);
        const manifest = mockAppFrameworks.find((m) => m.id === req.framework_id);
        const fwName = manifest?.display_name ?? req.framework_id;
        const created: AppInstance = {
            id,
            framework_id: req.framework_id,
            display_name: req.display_name || `${fwName} · ${id}`,
            placement: req.host_id === 'local' ? 'local_native' : 'remote_native',
            host_id: req.host_id,
            install_dir:
                req.install_dir ||
                (req.host_id === 'local'
                    ? `D:/NapCatQQ/apps/${req.framework_id}/${id}`
                    : `/home/ubuntu/ncd/apps/${req.framework_id}/${id}`),
            port: req.port ?? 20000 + Math.floor(Math.random() * 29152),
            state: 'not_installed',
            created_at_ms: Date.now(),
            install_renderer: req.install_renderer ?? true,
            origin: 'created',
            auto_start: true,
        };
        if (manifest?.webui_auth === 'user_password') {
            mockWebUiAccounts.set(id, {
                username: req.webui_username?.trim() || 'astrbot',
                password: req.webui_password || mockGeneratePassword(),
            });
        }
        instances = [...instances, created];
        emitMockEvent({ kind: 'app_instance_changed', instance: created, reason: 'created' });
        return withMockDelay(created);
    },

    probeProject: async (
        hostId: string,
        frameworkId: string,
        path: string,
    ): Promise<AppProjectProbe> => {
        const trimmed = path.trim();
        if (!trimmed) throw new Error('请填写项目目录');
        if (hostId.startsWith('remote:') && !trimmed.startsWith('/')) {
            throw new Error('远端路径必须是绝对路径');
        }
        if (/nope|not-a-project/i.test(trimmed)) {
            throw new Error('这里不是可导入的项目');
        }
        const name = trimmed.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? frameworkId;
        const isNonebot = frameworkId === 'nonebot2';
        const isAstrbot = frameworkId === 'astrbot';
        const emptyOnebot = /no-onebot|empty-platform/i.test(trimmed);
        const ambiguousOnebot = /ambiguous-onebot/i.test(trimmed);
        return withMockDelay({
            framework_id: frameworkId,
            path: trimmed,
            display_name: name,
            port: emptyOnebot || ambiguousOnebot
                ? undefined
                : isNonebot
                  ? 13120
                  : isAstrbot
                    ? 6199
                    : 7777,
            version: isNonebot ? '2.5.1' : isAstrbot ? '4.0.0' : '1.17.0',
            env_rel_path: isAstrbot ? 'data/cmd_config.json' : isNonebot ? '.env' : '.env',
            environment: isNonebot ? 'prod' : '',
            ready: true,
            running: hostId.startsWith('remote:'),
            supervisors: hostId.startsWith('remote:') && isNonebot ? ['bot-xiuxian'] : [],
            warnings: ambiguousOnebot
                ? ['有多条 OneBot v11（aiocqhttp），无法唯一认领。到 AstrBot WebUI 或原文指定要对接的那条']
                : emptyOnebot
                  ? ['还没有 OneBot v11，对接时会加一条']
                  : hostId.startsWith('remote:') && isNonebot
                    ? ['现在由 systemd 在跑（bot-xiuxian）。导入后改由这边开关，不要了可以还回去。']
                    : [],
            detected_bot_id: hostId.startsWith('remote:') && isNonebot ? '10001' : undefined,
        });
    },

    importInstance: async (req: ImportAppInstanceRequest): Promise<AppInstance> => {
        const probe = await mockAppFrameworkApi.probeProject(req.host_id, req.framework_id, req.path);
        const id = Math.random().toString(16).slice(2, 10);
        const imported: AppInstance = {
            id,
            framework_id: req.framework_id,
            display_name: req.display_name.trim() || probe.display_name,
            placement: req.host_id === 'local' ? 'local_native' : 'remote_native',
            host_id: req.host_id,
            install_dir: probe.path,
            port: probe.port ?? 0,
            state: probe.ready ? 'running' : 'not_installed',
            installed_version: probe.version,
            created_at_ms: Date.now(),
            install_renderer: false,
            origin: 'imported',
            auto_start: true,
            link: probe.detected_bot_id
                ? {
                      bot_id: probe.detected_bot_id,
                      mode: 'reverse_ws',
                      connection_name: 'ncd-adopt-forward',
                      linked_at_ms: Date.now(),
                  }
                : undefined,
        };
        instances = [...instances, imported];
        emitMockEvent({ kind: 'app_instance_changed', instance: imported, reason: 'imported' });
        return withMockDelay(imported);
    },

    install: async (id: string, version?: string | null): Promise<string> => {
        const inst = require(id);
        publish({ ...inst, state: 'installing' }, 'installing');
        return withMockDelay(simulateInstallTask(inst, version ?? null));
    },

    /** 真实链路只有 NeoBot 实现 available_versions，trait 默认 None；UI 拿到 null 就隐藏选择器 */
    listVersions: async (frameworkId: string): Promise<PackageVersions | null> => {
        if (frameworkId !== 'neobot') return withMockDelay(null);
        return withMockDelay({
            name: 'neobot-app',
            versions: [...MOCK_NEOBOT_VERSIONS],
            latest: MOCK_NEOBOT_VERSIONS[0] ?? null,
            has_prerelease: MOCK_NEOBOT_VERSIONS.some((v) => /[a-zA-Z]/.test(v.slice(1))),
        });
    },

    refresh: async (id: string): Promise<AppInstance> => withMockDelay(require(id)),

    tailLog: async (id: string, _lines = 1000): Promise<LogSnapshot> => {
        const lines = mockAppLogTail(require(id));
        return withMockDelay({ lines, total_lines: lines.length });
    },

    start: async (id: string): Promise<AppInstance> => {
        const next: AppInstance = { ...require(id), state: 'running', last_error: undefined };
        publish(next, 'started');
        playMockAppRun(next, () => instances.find((i) => i.id === id)?.state === 'running');
        return withMockDelay(next);
    },

    stop: async (id: string): Promise<AppInstance> => {
        const next: AppInstance = { ...require(id), state: 'stopped' };
        publish(next, 'stopped');
        return withMockDelay(next);
    },

    delete: async (id: string): Promise<void> => {
        instances = instances.filter((i) => i.id !== id);
        return withMockDelay(undefined);
    },

    pendingTerms: async (instanceId: string): Promise<AppPendingTerms[]> => {
        const inst = require(instanceId);
        const manifest = mockAppFrameworks.find((m) => m.id === inst.framework_id);
        if (!manifest?.terms.length || mockAcceptedTerms.has(instanceId)) return withMockDelay([]);
        return withMockDelay(
            manifest.terms.map((t) => ({ ...t, text: MOCK_TERMS_TEXT[t.id] ?? '' })),
        );
    },

    acceptTerms: async (instanceId: string): Promise<void> => {
        mockAcceptedTerms.add(instanceId);
        return withMockDelay(undefined);
    },

    previewLink: async (instanceId: string, botId: string): Promise<OneBotLinkPlan> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'maibot') {
            return withMockDelay({
                mode: 'forward_ws',
                instance_id: instanceId,
                bot_id: botId,
                connection: {
                    kind: 'ws_server',
                    host: '127.0.0.1',
                    port: 23456,
                    reportSelfMessage: false,
                    enableForcePushEvent: true,
                    heartInterval: 30000,
                    path: '/',
                    role: 'Universal',
                    enable: true,
                    name: `ncd-app:${instanceId}`,
                    messagePostFormat: 'array',
                    token: 'mockmockmockmockmockmock',
                    debug: false,
                },
                app_side_writes: [
                    {
                        path: 'plugins/MaiBot-Napcat-Adapter/config.toml',
                        summary: '启用适配器 / napcat_server 指向这条连接（host、port、token=<token>）',
                    },
                ],
                access_token: 'mockmockmockmockmockmock',
            });
        }
        if (inst.framework_id === 'yunzai') {
            return withMockDelay({
                mode: 'reverse_ws',
                instance_id: instanceId,
                bot_id: botId,
                connection: {
                    kind: 'ws_client',
                    url: `ws://127.0.0.1:${inst.port}/OneBotv11`,
                    reportSelfMessage: false,
                    heartInterval: 30000,
                    reconnectInterval: 30000,
                    role: 'Universal',
                    enable: true,
                    name: `ncd-app:${instanceId}`,
                    messagePostFormat: 'array',
                    token: 'mockmockmockmockmockmock',
                    debug: false,
                },
                app_side_writes: [
                    {
                        path: 'config/config/server.yaml',
                        summary: `port=${inst.port} / auth.Authorization=Bearer <token>`,
                    },
                ],
                access_token: 'mockmockmockmockmockmock',
            });
        }
        return withMockDelay({
            mode: 'reverse_ws',
            instance_id: instanceId,
            bot_id: botId,
            connection: {
                kind: 'ws_client',
                url: `ws://127.0.0.1:${inst.port}${
                    inst.framework_id === 'astrbot' ? '/ws' : inst.framework_id === 'koishi' ? '/onebot/ncd' : '/onebot/v11/ws'
                }`,
                reportSelfMessage: false,
                heartInterval: 30000,
                reconnectInterval: 30000,
                role: 'Universal',
                enable: true,
                name: `ncd-app:${instanceId}`,
                messagePostFormat: 'array',
                token: 'mockmockmockmockmockmock',
                debug: false,
            },
            app_side_writes:
                inst.framework_id === 'koishi'
                    ? [{ path: 'koishi.yml', summary: `adapter-onebot:ncd-link（selfId=${botId}，ws-reverse /onebot/ncd）` }]
                    : [
                          { path: '.env', summary: `HTTP_PORT=${inst.port} / WS_SERVER_AUTH_KEY=mock****` },
                          { path: '@karinjs/config/adapter.json', summary: '开启 onebot.ws_server.enable' },
                      ],
            access_token: 'mockmockmockmockmockmock',
        });
    },

    applyLink: async (instanceId: string, botId: string): Promise<AppInstance> => {
        const inst = require(instanceId);
        const forward = inst.framework_id === 'maibot';
        const next: AppInstance = {
            ...inst,
            link: {
                bot_id: botId,
                mode: forward ? 'forward_ws' : 'reverse_ws',
                connection_name: `ncd-app:${instanceId}`,
                linked_at_ms: Date.now(),
            },
        };
        if (forward) syncMaiBotLink(inst, true);
        else if (inst.framework_id === 'koishi') syncKoishiLink(inst, botId);
        else if (inst.framework_id === 'yunzai') syncYunzaiLink(inst, true);
        else syncKarinLinkToken(instanceId);
        publish(next, 'linked');
        return withMockDelay(next);
    },

    unlink: async (instanceId: string): Promise<AppInstance> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'maibot') syncMaiBotLink(inst, false);
        if (inst.framework_id === 'koishi') syncKoishiLink(inst, null);
        const next: AppInstance = { ...inst, link: undefined };
        publish(next, 'unlinked');
        return withMockDelay(next);
    },

    webui: async (instanceId: string, path?: string): Promise<AppInstanceWebUi> => {
        const inst = require(instanceId);
        const base =
            inst.framework_id === 'astrbot'
                ? `http://127.0.0.1:6185`
                : inst.framework_id === 'maibot' || inst.framework_id === 'koishi'
                  ? `http://127.0.0.1:${inst.port}/`
                  : `http://127.0.0.1:${inst.port}/web`;
        const suffix = path?.trim()
            ? path.startsWith('/')
                ? path
                : `/${path}`
            : '';
        return withMockDelay({
            url: `${base.replace(/\/$/, '')}${suffix}`,
            authKey:
                inst.framework_id === 'karin'
                    ? peekKarinHttpAuthKey(instanceId)
                    : inst.framework_id === 'maibot'
                      ? 'Ncd_mockMockMockMockMock'
                      : '',
            account: mockAccountView(inst) ?? undefined,
        });
    },

    webuiAccount: async (instanceId: string): Promise<AppWebUiAccount | null> =>
        withMockDelay(mockAccountView(require(instanceId))),

    resetWebUiPassword: async (
        instanceId: string,
        password: string | null,
    ): Promise<AppWebUiAccount> => {
        const inst = require(instanceId);
        if (inst.state === 'running') throw new Error('实例运行中，先停止再重置密码');
        const current = mockWebUiAccounts.get(instanceId) ?? { username: 'astrbot', password: null };
        mockWebUiAccounts.set(instanceId, {
            username: current.username,
            password: password?.trim() || mockGeneratePassword(),
        });
        const view = mockAccountView(inst);
        if (!view) throw new Error('该应用端不是账号密码登录');
        return withMockDelay(view);
    },

    setInstanceAutoStart: async (instanceId: string, autoStart: boolean): Promise<AppInstance> => {
        const next: AppInstance = { ...require(instanceId), auto_start: autoStart };
        publish(next, 'auto_start_changed');
        return withMockDelay(next);
    },

    ...createMockAppConfigApi({ require, publish }),

    listPluginConfigDocs: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppConfigDocument[]> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'astrbot') {
            const dir = pluginName.split('/').pop() ?? pluginName;
            return withMockDelay([
                {
                    id: `plugin:${dir}`,
                    label: `${dir}_config.json`,
                    rel_path: `data/config/${dir}_config.json`,
                    format: 'json',
                    hot_reload: false,
                },
            ]);
        }
        if (inst.framework_id === 'nonebot2') {
            return withMockDelay([
                {
                    id: 'env_prod',
                    label: '.env.prod',
                    rel_path: '.env.prod',
                    format: 'dot_env',
                    hot_reload: false,
                },
            ]);
        }
        if (inst.framework_id === 'yunzai') {
            // 单 JS 插件没有配置目录；目录插件列 config/ 下的 yaml
            if (pluginName.endsWith('.js')) return withMockDelay([]);
            return withMockDelay(
                ['config/cfg.yaml', 'config/profile.yaml'].map((rel) => ({
                    id: `plugin:${pluginName}:${rel}`,
                    label: rel,
                    rel_path: `plugins/${pluginName}/${rel}`,
                    format: 'yaml' as const,
                    hot_reload: true,
                })),
            );
        }
        if (inst.framework_id === 'maibot') {
            const dir = pluginName.replaceAll('.', '_');
            return withMockDelay([
                {
                    id: `plugin:${dir}`,
                    label: `${dir}/config.toml`,
                    rel_path: `plugins/${dir}/config.toml`,
                    format: 'toml',
                    hot_reload: true,
                },
            ]);
        }
        const dir = pluginName.replaceAll('/', '-');
        return withMockDelay([
            {
                id: `plugin:${pluginName}:config/config.json`,
                label: 'config.json',
                rel_path: `@karinjs/${dir}/config/config.json`,
                format: 'json',
                hot_reload: true,
            },
        ]);
    },

    pluginConfigSchema: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppPluginConfigSchema | null> => {
        const inst = require(instanceId);
        if (inst.framework_id !== 'astrbot') return withMockDelay(null);
        const dir = pluginName.split('/').pop() ?? pluginName;
        return withMockDelay({
            doc_id: `plugin:${dir}`,
            fields: [
                {
                    key: 'token',
                    kind: 'string',
                    label: 'Bot Token',
                    hint: '从上游平台复制',
                    obvious_hint: true,
                    secret: true,
                    options: [],
                    items: [],
                },
                {
                    key: 'mode',
                    kind: 'string',
                    label: '模式',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: ['chat', 'agent'],
                    items: [],
                },
                {
                    key: 'prompt',
                    kind: 'text',
                    label: '系统提示词',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [],
                },
                {
                    key: 'enabled_groups',
                    kind: 'list',
                    label: '启用的群',
                    hint: '留空表示全部',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [],
                },
                {
                    key: 'limits',
                    kind: 'object',
                    label: '限额',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [
                        {
                            key: 'per_user',
                            kind: 'int',
                            label: '每人每日',
                            hint: '',
                            obvious_hint: false,
                            secret: false,
                            options: [],
                            items: [],
                        },
                        {
                            key: 'strict',
                            kind: 'bool',
                            label: '超限直接拒绝',
                            hint: '',
                            obvious_hint: false,
                            secret: false,
                            options: [],
                            items: [],
                        },
                    ],
                },
                {
                    key: 'extra',
                    kind: 'json',
                    label: '附加参数',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [],
                },
            ],
        });
    },

    listPluginMarket: () => withMockDelay(mockPluginMarket.slice()),

    listStore: async (frameworkId: string, resource: AppStoreResource): Promise<AppStoreMarketEntry[]> => {
        if (frameworkId === 'karin' && resource === 'plugin') {
            return withMockDelay(mockPluginMarket.map(karinToStore));
        }
        if (frameworkId === 'nonebot2' && resource === 'adapter') {
            return withMockDelay(mockNoneBotAdapters.slice());
        }
        if (frameworkId === 'nonebot2' && resource === 'plugin') {
            return withMockDelay(mockNoneBotPlugins.slice());
        }
        if (frameworkId === 'astrbot' && resource === 'plugin') {
            return withMockDelay(mockAstrBotPlugins.slice());
        }
        if (frameworkId === 'maibot' && resource === 'plugin') {
            return withMockDelay(mockMaiBotPlugins.slice());
        }
        if (frameworkId === 'koishi' && resource === 'plugin') {
            return withMockDelay(mockKoishiPlugins.slice());
        }
        if (frameworkId === 'yunzai' && resource === 'plugin') {
            return withMockDelay(mockYunzaiPlugins.slice());
        }
        return withMockDelay([]);
    },

    listStoreInstalled: async (
        instanceId: string,
        resource: AppStoreResource,
    ): Promise<AppStoreInstalled[]> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'koishi') {
            const cfg = peekKoishiConfig(inst);
            const on = new Set(walk(cfg.plugins).filter((n) => n.enabled).map((n) => n.name));
            return withMockDelay(
                koishiMockPackages(cfg).map((p) => ({
                    id: p.package,
                    name: p.name,
                    resource: 'plugin' as const,
                    flavor: 'npm' as const,
                    version: p.version ?? undefined,
                    enabled: on.has(p.name),
                    package: p.package,
                    locked: ['server', 'console', 'config', 'market', 'logger', 'adapter-onebot'].includes(p.name),
                })),
            );
        }
        return withMockDelay(mockStoreInstalledFor(instanceId, resource));
    },

    listPlugins: async (instanceId: string): Promise<KarinPluginInstalled[]> => {
        require(instanceId);
        return withMockDelay(mockInstalledFor(instanceId));
    },

    submitPluginOp: async (
        instanceId: string,
        pluginName: string,
        action: AppPluginAction,
        resource?: AppStoreResource,
    ): Promise<string> => {
        const target = require(instanceId);
        if (target.framework_id === 'koishi') {
            applyMockKoishiStoreOp(target, pluginName, action);
            return withMockDelay(`mock-plugin-${instanceId}-${pluginName}`);
        }
        if (STORE_MOCK_FRAMEWORKS.has(target.framework_id)) {
            applyMockStoreOp(instanceId, pluginName, action, resource ?? 'plugin');
        } else {
            applyMockPluginOp(instanceId, pluginName, action);
        }
        return withMockDelay(`mock-plugin-${instanceId}-${pluginName}`);
    },

    setPluginEnabled: async (
        instanceId: string,
        pluginName: string,
        enabled: boolean,
        _overwrite?: boolean,
        resource?: AppStoreResource,
    ): Promise<AppConfigWriteResult> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'koishi') {
            const short = koishiShortName(pluginName);
            editKoishiConfig(inst, (cfg) => {
                let next = cfg;
                if (!walk(cfg.plugins).some((n) => n.name === short)) {
                    next = appendTo(next, [], newPlugin(next, short, enabled));
                }
                const flip = (list: KoishiInstanceConfig['plugins']): KoishiInstanceConfig['plugins'] =>
                    list.map((n) => ({ ...n, enabled: n.name === short ? enabled : n.enabled, children: flip(n.children) }));
                return { ...next, plugins: flip(next.plugins) };
            });
            return withMockDelay({
                config: { framework: 'koishi', data: peekKoishiConfig(inst) },
                revision: 'mock-r-plugin',
                documents: [],
                restart_required: false,
                relinked: false,
                port_changed: false,
            });
        }
        if (STORE_MOCK_FRAMEWORKS.has(inst.framework_id)) {
            const key = storeKey(instanceId, resource ?? 'plugin');
            const list = mockStoreInstalledFor(instanceId, resource ?? 'plugin');
            mockStoreInstalled.set(
                key,
                list.map((p) => (p.id === pluginName || p.name === pluginName ? { ...p, enabled } : p)),
            );
            if (inst.framework_id === 'yunzai') {
                // 单 JS 插件改名成 .js.disabled，云崽自己热卸载；没有配置文件要写
                return withMockDelay({
                    config: { framework: 'yunzai', data: yunzaiDefaultConfig(inst.port) },
                    revision: 'mock-r-plugin',
                    documents: [],
                    restart_required: false,
                    relinked: false,
                    port_changed: false,
                });
            }
            if (inst.framework_id === 'astrbot') {
                const config = astrbotDefaultConfig(inst.port);
                return withMockDelay({
                    config: { framework: 'astrbot', data: config },
                    revision: 'mock-r-plugin',
                    documents: [],
                    restart_required: inst.state === 'running',
                    relinked: false,
                    port_changed: false,
                });
            }
            const config = nonebot2DefaultConfig(inst.port);
            return withMockDelay({
                config: { framework: 'nonebot2', data: config },
                revision: 'mock-r-plugin',
                documents: [],
                restart_required: inst.state === 'running',
                relinked: false,
                port_changed: false,
            });
        }
        const list = mockInstalledFor(instanceId);
        const next = list.map((p) => (p.name === pluginName ? { ...p, enabled } : p));
        mockInstalled.set(instanceId, next);
        const config = karinDefaultConfig(inst.port);
        return withMockDelay({
            config: { framework: 'karin', data: config },
            revision: 'mock-r-plugin',
            documents: [],
            restart_required: false,
            relinked: false,
            port_changed: false,
        });
    },

    astrbotDashboardStatus: (instanceId: string): Promise<AstrBotDashboardStatus> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.status(inst, mockAccountView(inst));
    },
    astrbotListPersonas: (instanceId: string): Promise<AstrBotPersona[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listPersonas(inst, mockAccountView(inst));
    },
    astrbotUpsertPersona: (
        instanceId: string,
        persona: AstrBotPersona,
        creating: boolean,
    ): Promise<AstrBotPersona[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.upsertPersona(inst, mockAccountView(inst), persona, creating);
    },
    astrbotDeletePersona: (instanceId: string, personaId: string): Promise<AstrBotPersona[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deletePersona(inst, mockAccountView(inst), personaId);
    },
    astrbotListKbs: (instanceId: string): Promise<AstrBotKnowledgeBase[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listKbs(inst, mockAccountView(inst));
    },
    astrbotCreateKb: (
        instanceId: string,
        request: AstrBotKbCreate,
    ): Promise<AstrBotKnowledgeBase[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.createKb(inst, mockAccountView(inst), request);
    },
    astrbotDeleteKb: (instanceId: string, kbId: string): Promise<AstrBotKnowledgeBase[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deleteKb(inst, mockAccountView(inst), kbId);
    },
    astrbotListSessionRules: (instanceId: string): Promise<AstrBotSessionRule[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listRules(inst, mockAccountView(inst));
    },
    astrbotUpdateSessionRule: (
        instanceId: string,
        rule: AstrBotSessionRule,
    ): Promise<AstrBotSessionRule[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.updateRule(inst, mockAccountView(inst), rule);
    },
    astrbotDeleteSessionRule: (
        instanceId: string,
        umo: string,
        ruleKey: string,
    ): Promise<AstrBotSessionRule[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deleteRule(inst, mockAccountView(inst), umo, ruleKey);
    },
    astrbotListAbconfs: (instanceId: string): Promise<AstrBotAbconfInfo[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listAbconfs(inst, mockAccountView(inst));
    },
    astrbotCreateAbconf: (instanceId: string, name: string): Promise<AstrBotAbconfInfo[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.createAbconf(inst, mockAccountView(inst), name);
    },
    astrbotDeleteAbconf: (instanceId: string, abconfId: string): Promise<AstrBotAbconfInfo[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deleteAbconf(inst, mockAccountView(inst), abconfId);
    },
    astrbotListSourceModels: (instanceId: string, sourceId: string): Promise<string[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listSourceModels(inst, mockAccountView(inst), sourceId);
    },
    astrbotListSubagentTools: (instanceId: string): Promise<string[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listSubagentTools(inst, mockAccountView(inst));
    },

    koishiStatus: (instanceId: string): Promise<KoishiRuntimeStatus> => {
        const inst = require(instanceId);
        return koishiMockStatus(inst, peekKoishiConfig(inst));
    },
    koishiPluginSchemas: (instanceId: string, names: string[]): Promise<KoishiPluginSchema[]> => {
        require(instanceId);
        return withMockDelay(koishiMockSchemas(names));
    },
    koishiPackages: (instanceId: string): Promise<KoishiPackageInfo[]> =>
        withMockDelay(koishiMockPackages(peekKoishiConfig(require(instanceId)))),
    koishiRestart: async (instanceId: string): Promise<void> => {
        const inst = require(instanceId);
        if (inst.state !== 'running') throw new Error('Koishi 没在运行');
        await withMockDelay(undefined);
        playMockAppRun(inst, () => require(instanceId).state === 'running');
    },
    koishiSandboxSend: (instanceId: string, msg: { platform: string; user: string; channel: string; content: string }) =>
        koishiMockSandboxSend(instanceId, msg),
    koishiSandboxMessages: (instanceId: string) => koishiMockSandboxMessages(instanceId),
    koishiExplorerTree: (instanceId: string) => koishiMockExplorerTree(instanceId),
    koishiExplorerRead: (instanceId: string, path: string) => koishiMockExplorerRead(instanceId, path),
    koishiExplorerWrite: (instanceId: string, path: string, content: string, binary?: boolean) =>
        koishiMockExplorerWrite(instanceId, path, content, binary),
    koishiExplorerMkdir: (instanceId: string, path: string) => koishiMockExplorerMkdir(instanceId, path),
    koishiExplorerRemove: (instanceId: string, path: string) => koishiMockExplorerRemove(instanceId, path),
    koishiExplorerRename: (instanceId: string, from: string, to: string) =>
        koishiMockExplorerRename(instanceId, from, to),
    koishiDatabaseTables: (_instanceId: string) => koishiMockDatabaseTables(),
    koishiDatabaseRows: (_instanceId: string, table: string, offset: number, limit: number) =>
        koishiMockDatabaseRows(table, offset, limit),
    koishiCommands: (instanceId: string) => koishiMockCommands(instanceId),
    koishiCommandUpdate: (instanceId: string, name: string, config: Record<string, unknown>) =>
        koishiMockCommandUpdate(instanceId, name, config),
    koishiCommandAliases: (instanceId: string, name: string, aliases: string[]) =>
        koishiMockCommandAliases(instanceId, name, aliases),

    maibotStatus: (instanceId: string): Promise<MaiBotRuntimeStatus> => mockMaiBotRuntime.status(require(instanceId)),
    maibotRestart: async (instanceId: string): Promise<void> => {
        await mockMaiBotRuntime.restart(require(instanceId));
        playMockAppRun(require(instanceId), () => require(instanceId).state === 'running');
    },
    maibotStats: (instanceId: string, hours: number): Promise<MaiBotStatsSummary> =>
        mockMaiBotRuntime.stats(require(instanceId), hours),
    maibotChatSessions: (instanceId: string): Promise<MaiBotChatSession[]> =>
        mockMaiBotRuntime.chatSessions(require(instanceId)),
    maibotProviderModels: (instanceId: string, provider: MaiBotAPIProvider): Promise<MaiBotProviderModel[]> =>
        mockMaiBotRuntime.providerModels(require(instanceId), provider),
    maibotTestProvider: (instanceId: string, provider: MaiBotAPIProvider): Promise<MaiBotProviderCheck> =>
        mockMaiBotRuntime.testProvider(require(instanceId), provider),
    maibotMcpStatus: (instanceId: string): Promise<MaiBotMcpStatus> => mockMaiBotRuntime.mcpStatus(require(instanceId)),
    maibotTestMcp: (instanceId: string, server: MaiBotMCPServerItemConfig): Promise<MaiBotMcpTest> =>
        mockMaiBotRuntime.testMcp(require(instanceId), server),
};

const mockPluginMarket: KarinPluginMarketEntry[] = [
    {
        name: '@karinjs/plugin-basic',
        type: 'npm',
        description: 'Karin 基础插件',
        time: '2025-01-19 10:00:00',
        home: 'https://github.com/karinjs/karin-plugin-basic',
        author: [{ name: 'shijin', home: 'https://github.com/sj817' }],
        repo: [
            {
                url: 'https://github.com/karinjs/karin-plugin-basic',
                type: 'github',
                branch: 'main',
            },
        ],
        files: [],
        allowBuild: [],
    },
    {
        name: 'karin-plugin-example-git',
        type: 'git',
        description: '示例 git 插件',
        time: '2025-03-01 12:00:00',
        home: 'https://github.com/karinjs/karin-plugin-example',
        author: [{ name: 'KarinJS', home: 'https://github.com/KarinJS' }],
        repo: [
            {
                url: 'https://github.com/karinjs/karin-plugin-example',
                type: 'github',
                branch: 'main',
            },
        ],
        files: [],
        allowBuild: [],
    },
    {
        name: '@karinjs/plugin-puppeteer',
        type: 'npm',
        description: '插件版渲染器',
        time: '2025-04-01 09:00:00',
        home: 'https://github.com/karinjs/plugin-puppeteer',
        author: [{ name: 'KarinJS', home: 'https://github.com/KarinJS' }],
        repo: [
            {
                url: 'https://github.com/karinjs/plugin-puppeteer',
                type: 'github',
                branch: 'main',
            },
        ],
        files: [],
        allowBuild: [],
    },
];

const mockInstalled = new Map<string, KarinPluginInstalled[]>();

function mockInstalledFor(instanceId: string): KarinPluginInstalled[] {
    if (!mockInstalled.has(instanceId)) {
        mockInstalled.set(instanceId, [
            {
                name: '@karinjs/plugin-puppeteer',
                kind: 'npm',
                version: '1.2.0',
                enabled: true,
            },
        ]);
    }
    return mockInstalled.get(instanceId) ?? [];
}

function karinToStore(entry: KarinPluginMarketEntry): AppStoreMarketEntry {
    return {
        resource: 'plugin',
        id: entry.name,
        name: entry.name,
        description: entry.description,
        version: '',
        author: entry.author[0]?.name ?? '',
        homepage: entry.home,
        time: entry.time,
        package: entry.name,
        module_name: entry.name,
        flavor: entry.type === 'git' ? 'git' : entry.type === 'app' ? 'app' : 'npm',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: entry.author,
        repos: entry.repo,
        files: entry.files,
        allow_build: entry.allowBuild,
    };
}

const mockNoneBotAdapters: AppStoreMarketEntry[] = [
    {
        resource: 'adapter',
        id: 'nonebot.adapters.onebot.v11',
        name: 'OneBot V11',
        description: 'OneBot 协议',
        version: '2.4.6',
        author: 'yanyongyu',
        homepage: 'https://onebot.adapters.nonebot.dev',
        time: '',
        package: 'nonebot-adapter-onebot',
        module_name: 'nonebot.adapters.onebot.v11',
        flavor: 'pypi',
        is_official: true,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
    {
        resource: 'adapter',
        id: 'nonebot.adapters.console',
        name: 'Console',
        description: '控制台适配器',
        version: '',
        author: '',
        homepage: '',
        time: '',
        package: 'nonebot-adapter-console',
        module_name: 'nonebot.adapters.console',
        flavor: 'pypi',
        is_official: true,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
    {
        resource: 'adapter',
        id: 'nonebot.adapters.onebot.v12',
        name: 'OneBot V12',
        description: 'OneBot V12，与 V11 共用 nonebot-adapter-onebot',
        version: '',
        author: 'yanyongyu',
        homepage: '',
        time: '',
        package: 'nonebot-adapter-onebot',
        module_name: 'nonebot.adapters.onebot.v12',
        flavor: 'pypi',
        is_official: true,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
];

const mockNoneBotPlugins: AppStoreMarketEntry[] = [
    {
        resource: 'plugin',
        id: 'nonebot_plugin_status',
        name: 'Status',
        description: '运行状态',
        version: '',
        author: '',
        homepage: '',
        time: '',
        package: 'nonebot-plugin-status',
        module_name: 'nonebot_plugin_status',
        flavor: 'pypi',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: ['nonebot.adapters.onebot.v11'],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
    {
        resource: 'plugin',
        id: 'nonebot_plugin_htmlrender',
        name: 'htmlrender',
        description: 'HTML 渲染',
        version: '',
        author: '',
        homepage: '',
        time: '',
        package: 'nonebot-plugin-htmlrender',
        module_name: 'nonebot_plugin_htmlrender',
        flavor: 'pypi',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
];

const mockAstrBotPlugins: AppStoreMarketEntry[] = [
    {
        resource: 'plugin',
        id: 'soulter/helloworld',
        name: 'helloworld',
        description: '示例插件',
        version: '1.2.0',
        author: 'soulter',
        homepage: 'https://github.com/Soulter/helloworld',
        time: '',
        package: 'https://github.com/Soulter/helloworld',
        module_name: '',
        flavor: 'git',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: ['aiocqhttp'],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
];

const maibotMarketEntry = (
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

const mockMaiBotPlugins: AppStoreMarketEntry[] = [
    maibotMarketEntry('maibot-team.napcat-adapter', 'Napcat_Adapter 适配器', '插件版 Napcat 适配器，提供与 Napcat 的连接功能。', 'MaiBot Team', 'Mai-with-u/MaiBot-Napcat-Adapter'),
    maibotMarketEntry('sengokucola.mute-plugin', '群聊禁言管理插件', '智能禁言和手动禁言命令', 'SengokuCola', 'SengokuCola/MutePlugin'),
    maibotMarketEntry('a0000xz.maibot-tarots-plugin', '塔罗牌插件', '抽一张塔罗牌，麦麦来解读', 'A0000Xz', 'A0000Xz/MaiBot-Tarots-Plugin'),
];

const koishiMarketEntry = (
    pkg: string,
    description: string,
    author: string,
    tags: string[],
    version = '1.0.0',
): AppStoreMarketEntry => ({
    resource: 'plugin',
    id: pkg,
    name: koishiShortName(pkg),
    description,
    version,
    author,
    homepage: `https://www.npmjs.com/package/${pkg}`,
    time: '2026-06-05T06:17:20.210Z',
    package: pkg,
    module_name: koishiShortName(pkg),
    flavor: 'npm',
    is_official: pkg.startsWith('@koishijs/'),
    valid: true,
    tags,
    supported_adapters: [],
    authors: [],
    repos: [],
    files: [],
    allow_build: [],
});

const mockKoishiPlugins: AppStoreMarketEntry[] = [
    koishiMarketEntry('koishi-plugin-adapter-onebot', 'OneBot 适配器', 'shigma', ['适配器', 'onebot'], '6.9.4'),
    koishiMarketEntry('koishi-plugin-echo', '复读消息', 'shigma', ['实用工具'], '2.2.5'),
    koishiMarketEntry('koishi-plugin-chatluna', '多平台模型接入的大语言模型聊天服务', 'dingyi222666', ['人工智能'], '1.3.0'),
    koishiMarketEntry('koishi-plugin-puppeteer', '网页截图和图片渲染服务', 'shigma', ['扩展功能'], '3.9.0'),
    koishiMarketEntry('@koishijs/plugin-help', '帮助指令', 'shigma', ['实用工具'], '2.4.6'),
];

/** 形状照后端 yunzai::store::parse_index：id 是 plugins/ 下的目录名，单 JS 是文件名 */
const yunzaiEntry = (
    id: string,
    name: string,
    description: string,
    author: string,
    home: string,
    tags: string[],
    extra: Partial<AppStoreMarketEntry> = {},
): AppStoreMarketEntry => ({
    resource: 'plugin',
    id,
    name,
    description,
    version: '',
    author,
    homepage: home,
    time: '',
    package: id,
    module_name: id,
    flavor: 'git',
    is_official: tags.includes('推荐'),
    valid: true,
    tags,
    supported_adapters: [],
    authors: [{ name: author, home: '' }],
    repos: [{ url: home, type: 'git', branch: '' }],
    files: [],
    allow_build: [],
    ...extra,
});

const mockYunzaiPlugins: AppStoreMarketEntry[] = [
    yunzaiEntry('genshin', '原神基础 (genshin)', 'TRSS 版原神基础功能，装喵喵插件前先装它', '时雨🌌星空', 'https://github.com/TimeRainStarSky/Yunzai-genshin', ['推荐']),
    yunzaiEntry('TRSS-Plugin', 'TRSS 插件 (TRSS-Plugin)', 'TRSS 自带的工具箱：远程命令、文件操作、语音合成等', '时雨🌌星空', 'https://github.com/TimeRainStarSky/TRSS-Plugin', ['推荐']),
    yunzaiEntry('miao-plugin', '喵喵插件 (miao-plugin)', '原神、星铁角色面板、伤害计算、抽卡统计', 'yoimiya-kokomi', 'https://gitee.com/yoimiya-kokomi/miao-plugin', ['推荐', '游戏']),
    yunzaiEntry('xiaoyao-cvs-plugin', '逍遥图鉴', '原神图鉴、攻略、签到', 'Ctrlcvs', 'https://gitee.com/Ctrlcvs/xiaoyao-cvs-plugin', ['游戏']),
    yunzaiEntry('earth-k-plugin', '土块插件', '点歌、AI 绘图、表情包合成', 'SmallK111407', 'https://gitee.com/SmallK111407/earth-k-plugin', ['功能']),
    yunzaiEntry('xiuxian-plugin', '修仙文游', '群里一起修仙的文字游戏', 'ningmengchongshui', 'https://gitee.com/ningmengchongshui/xiuxian-plugin', ['文游']),
    yunzaiEntry('link:某网盘插件', '某网盘插件', '主页不是仓库，只能照说明手动装', '佚名', 'https://example.com/plugin', ['功能'], {
        valid: false,
        repos: [],
    }),
    yunzaiEntry('chuo.js', '戳一戳回复', '被戳的时候随机回一句', 'Pinging', 'https://gitee.com/Pinging/js-plugin', ['单 JS'], {
        flavor: 'app',
        repos: [],
        files: [{ name: 'chuo.js', url: 'https://gitee.com/Pinging/js-plugin/raw/master/chuo.js', description: '' }],
    }),
    yunzaiEntry('qianwen.js', '通义千问', '接通义千问聊天，要自己填 API Key', 'Lain', 'https://gitee.com/Lain/js', ['单 JS'], {
        flavor: 'app',
        repos: [],
        files: [{ name: 'qianwen.js', url: 'https://gitee.com/Lain/js/raw/main/qianwen.js', description: '' }],
    }),
];

/** 这几个框架的商店 mock 走通用的已装表（Karin、麦麦走各自的） */
const STORE_MOCK_FRAMEWORKS = new Set(['nonebot2', 'astrbot', 'yunzai']);

const mockStoreInstalled = new Map<string, AppStoreInstalled[]>();

function storeKey(instanceId: string, resource: AppStoreResource): string {
    return `${instanceId}:${resource}`;
}

function seedStoreInstalled(framework: string | undefined, resource: AppStoreResource): AppStoreInstalled[] {
    if (framework === 'yunzai') {
        // 装了 TRSS 插件和一个停着的单 JS，没装 genshin / 喵喵：概览会提示去装
        return resource === 'plugin'
            ? [
                  {
                      id: 'TRSS-Plugin',
                      name: 'TRSS-Plugin',
                      resource: 'plugin',
                      flavor: 'git',
                      version: '1.0.0',
                      enabled: true,
                      package: 'TRSS-Plugin',
                      locked: false,
                  },
                  {
                      id: 'chuo.js',
                      name: 'chuo.js',
                      resource: 'plugin',
                      flavor: 'app',
                      enabled: false,
                      package: 'chuo.js',
                      locked: false,
                  },
              ]
            : [];
    }
    if (framework === 'maibot') {
        return resource === 'plugin'
            ? [
                  {
                      id: 'maibot-team.napcat-adapter',
                      name: 'Napcat_Adapter 适配器',
                      resource: 'plugin',
                      flavor: 'git',
                      version: '1.4.0',
                      enabled: true,
                      package: 'https://github.com/Mai-with-u/MaiBot-Napcat-Adapter',
                      locked: true,
                  },
                  {
                      id: 'maibot-team.hello-world-plugin',
                      name: 'Hello World 示例插件',
                      resource: 'plugin',
                      flavor: 'git',
                      version: '2.0.0',
                      enabled: false,
                      package: '',
                      locked: false,
                  },
              ]
            : [];
    }
    if (resource === 'adapter') {
        return framework === 'astrbot'
            ? []
            : [
                  {
                      id: 'nonebot.adapters.onebot.v11',
                      name: 'OneBot V11',
                      resource: 'adapter',
                      flavor: 'pypi',
                      version: '2.4.6',
                      enabled: true,
                      package: 'nonebot-adapter-onebot',
                      locked: false,
                  },
              ];
    }
    return framework === 'astrbot'
        ? [
              {
                  id: 'soulter/helloworld',
                  name: 'helloworld',
                  resource: 'plugin',
                  flavor: 'git',
                  version: '1.2.0',
                  enabled: true,
                  package: 'https://github.com/Soulter/helloworld',
                  locked: false,
              },
          ]
        : [
              {
                  id: 'nonebot_plugin_status',
                  name: 'Status',
                  resource: 'plugin',
                  flavor: 'pypi',
                  version: '0.9.0',
                  enabled: true,
                  package: 'nonebot-plugin-status',
                  locked: false,
              },
          ];
}

function mockStoreInstalledFor(instanceId: string, resource: AppStoreResource): AppStoreInstalled[] {
    const key = storeKey(instanceId, resource);
    if (!mockStoreInstalled.has(key)) {
        const inst = instances.find((i) => i.id === instanceId);
        mockStoreInstalled.set(key, seedStoreInstalled(inst?.framework_id, resource));
    }
    return mockStoreInstalled.get(key) ?? [];
}

/** 装 = 插件树里加一条停用的；卸 = 所有同名条目摘掉（真机后端还要 yarn add / remove） */
function applyMockKoishiStoreOp(inst: AppInstance, pluginName: string, action: AppPluginAction) {
    const short = koishiShortName(pluginName);
    editKoishiConfig(inst, (cfg) => {
        if (action === 'uninstall') {
            const drop = (list: KoishiInstanceConfig['plugins']): KoishiInstanceConfig['plugins'] =>
                list.filter((n) => n.name !== short).map((n) => ({ ...n, children: drop(n.children) }));
            return { ...cfg, plugins: drop(cfg.plugins) };
        }
        if (walk(cfg.plugins).some((n) => n.name === short)) return cfg;
        return appendTo(cfg, [], newPlugin(cfg, short, false));
    });
}

function applyMockStoreOp(
    instanceId: string,
    pluginName: string,
    action: AppPluginAction,
    resource: AppStoreResource,
) {
    const key = storeKey(instanceId, resource);
    const current = mockStoreInstalledFor(instanceId, resource);
    if (action === 'uninstall') {
        mockStoreInstalled.set(
            key,
            current.filter((p) => p.id !== pluginName && p.name !== pluginName),
        );
        return;
    }
    if (current.some((p) => p.id === pluginName || p.name === pluginName)) return;
    const market = [
        ...(resource === 'adapter' ? mockNoneBotAdapters : mockNoneBotPlugins),
        ...mockAstrBotPlugins,
        ...mockMaiBotPlugins,
        ...mockYunzaiPlugins,
    ].find((e) => e.id === pluginName || e.name === pluginName);
    mockStoreInstalled.set(key, [
        ...current,
        {
            id: market?.id ?? pluginName,
            name: market?.name ?? pluginName,
            resource,
            flavor: market?.flavor ?? 'pypi',
            version: '1.0.0',
            enabled: true,
            package: market?.package ?? '',
            locked: false,
        },
    ]);
}

function applyMockPluginOp(instanceId: string, pluginName: string, action: AppPluginAction) {
    const current = mockInstalledFor(instanceId);
    const market = mockPluginMarket.find((e) => e.name === pluginName);
    if (action === 'uninstall') {
        mockInstalled.set(
            instanceId,
            current.filter((p) => p.name !== pluginName),
        );
        return;
    }
    if (current.some((p) => p.name === pluginName)) return;
    mockInstalled.set(instanceId, [
        ...current,
        {
            name: pluginName,
            kind: market?.type ?? 'npm',
            version: action === 'update' ? 'latest' : '1.0.0',
            enabled: true,
        },
    ]);
}

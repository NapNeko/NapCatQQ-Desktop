// 浏览器预览模式下的应用端（Karin 等）假数据。
// 真 IPC 实装在 core/services/app-framework.service.ts。

import type {
    AppConfigDocument,
    AppConfigWriteResult,
    AppFrameworkManifest,
    AppInstance,
    AppInstanceWebUi,
    AppPluginAction,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
    CreateAppInstanceRequest,
    KarinPluginInstalled,
    KarinPluginMarketEntry,
    OneBotLinkPlan,
} from '../types';
import { karinDefaultConfig } from '../../domain/apps/karinConfig';
import { nonebot2DefaultConfig } from '../../domain/apps/nonebot2Config';
import { emitMockEvent } from './events.mock';
import { withMockDelay } from './bootstrap.mock';
import { createMockAppConfigApi, peekKarinHttpAuthKey, syncKarinLinkToken } from './app-config.mock';

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
    },
];

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
    },
    {
        id: 'n9b8c7d6',
        framework_id: 'nonebot2',
        display_name: 'NoneBot2 · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/nonebot2/n9b8c7d6',
        port: 8080,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:n9b8c7d6',
            linked_at_ms: Date.now() - 1_800_000,
        },
        installed_version: '2.4.2',
        created_at_ms: Date.now() - 7_200_000,
        install_renderer: false,
    },
];

function publish(instance: AppInstance, reason: string) {
    instances = instances.map((i) => (i.id === instance.id ? instance : i));
    emitMockEvent({ kind: 'app_instance_changed', instance, reason });
}

function require(id: string): AppInstance {
    const found = instances.find((i) => i.id === id);
    if (!found) throw new Error(`应用实例不存在: ${id}`);
    return found;
}

export const mockAppFrameworkApi = {
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
        };
        instances = [...instances, created];
        emitMockEvent({ kind: 'app_instance_changed', instance: created, reason: 'created' });
        return withMockDelay(created);
    },

    install: async (id: string): Promise<string> => {
        const inst = require(id);
        publish({ ...inst, state: 'installing' }, 'installing');
        setTimeout(() => {
            publish(
                { ...require(id), state: 'installed', installed_version: '1.17.0' },
                'installed',
            );
        }, 2500);
        return withMockDelay(`mock-install-${id}`);
    },

    refresh: async (id: string): Promise<AppInstance> => withMockDelay(require(id)),

    start: async (id: string): Promise<AppInstance> => {
        const next: AppInstance = { ...require(id), state: 'running', last_error: undefined };
        publish(next, 'started');
        let n = 0;
        const timer = setInterval(() => {
            const cur = instances.find((i) => i.id === id);
            if (!cur || cur.state !== 'running' || n++ > 20) {
                clearInterval(timer);
                return;
            }
            emitMockEvent({
                kind: 'app_instance_log_appended',
                instance_id: id,
                line: `[Karin][INFO] heartbeat #${n} · ws server listening on :${cur.port}`,
            });
        }, 2000);
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

    previewLink: async (instanceId: string, botId: string): Promise<OneBotLinkPlan> => {
        const inst = require(instanceId);
        return withMockDelay({
            mode: 'reverse_ws',
            instance_id: instanceId,
            bot_id: botId,
            connection: {
                url: `ws://127.0.0.1:${inst.port}/onebot/v11/ws`,
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
                { path: '.env', summary: `HTTP_PORT=${inst.port} / WS_SERVER_AUTH_KEY=mock****` },
                { path: '@karinjs/config/adapter.json', summary: '开启 onebot.ws_server.enable' },
            ],
            access_token: 'mockmockmockmockmockmock',
        });
    },

    applyLink: async (instanceId: string, botId: string): Promise<AppInstance> => {
        const next: AppInstance = {
            ...require(instanceId),
            link: {
                bot_id: botId,
                mode: 'reverse_ws',
                connection_name: `ncd-app:${instanceId}`,
                linked_at_ms: Date.now(),
            },
        };
        syncKarinLinkToken(instanceId);
        publish(next, 'linked');
        return withMockDelay(next);
    },

    unlink: async (instanceId: string): Promise<AppInstance> => {
        const next: AppInstance = { ...require(instanceId), link: undefined };
        publish(next, 'unlinked');
        return withMockDelay(next);
    },

    webui: async (instanceId: string): Promise<AppInstanceWebUi> => {
        const inst = require(instanceId);
        return withMockDelay({
            url: `http://127.0.0.1:${inst.port}/web`,
            authKey: peekKarinHttpAuthKey(instanceId),
        });
    },

    ...createMockAppConfigApi({ require, publish }),

    listPluginConfigDocs: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppConfigDocument[]> => {
        const inst = require(instanceId);
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
        return withMockDelay([]);
    },

    listStoreInstalled: async (
        instanceId: string,
        resource: AppStoreResource,
    ): Promise<AppStoreInstalled[]> => {
        require(instanceId);
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
        require(instanceId);
        if (require(instanceId).framework_id === 'nonebot2') {
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
        if (inst.framework_id === 'nonebot2') {
            const key = storeKey(instanceId, resource ?? 'plugin');
            const list = mockStoreInstalledFor(instanceId, resource ?? 'plugin');
            mockStoreInstalled.set(
                key,
                list.map((p) => (p.id === pluginName || p.name === pluginName ? { ...p, enabled } : p)),
            );
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

const mockStoreInstalled = new Map<string, AppStoreInstalled[]>();

function storeKey(instanceId: string, resource: AppStoreResource): string {
    return `${instanceId}:${resource}`;
}

function mockStoreInstalledFor(instanceId: string, resource: AppStoreResource): AppStoreInstalled[] {
    const key = storeKey(instanceId, resource);
    if (!mockStoreInstalled.has(key)) {
        mockStoreInstalled.set(
            key,
            resource === 'adapter'
                ? [
                      {
                          id: 'nonebot.adapters.onebot.v11',
                          name: 'OneBot V11',
                          resource: 'adapter',
                          flavor: 'pypi',
                          version: '2.4.6',
                          enabled: true,
                          package: 'nonebot-adapter-onebot',
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
                      },
                  ],
        );
    }
    return mockStoreInstalled.get(key) ?? [];
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
    const market = (resource === 'adapter' ? mockNoneBotAdapters : mockNoneBotPlugins).find(
        (e) => e.id === pluginName || e.name === pluginName,
    );
    mockStoreInstalled.set(key, [
        ...current,
        {
            id: market?.id ?? pluginName,
            name: market?.name ?? pluginName,
            resource,
            flavor: 'pypi',
            version: '1.0.0',
            enabled: true,
            package: market?.package ?? '',
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

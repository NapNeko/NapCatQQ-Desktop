// Karin 自己的插件市场与已装表（形状照 KarinPluginMarketEntry），以及 Karin 的装卸写入。
import type {
    AppPluginAction,
    AppStoreMarketEntry,
    KarinPluginInstalled,
    KarinPluginMarketEntry,
} from '../../types';

export const mockPluginMarket: KarinPluginMarketEntry[] = [
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

export const mockInstalled = new Map<string, KarinPluginInstalled[]>();

export function mockInstalledFor(instanceId: string): KarinPluginInstalled[] {
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

export function karinToStore(entry: KarinPluginMarketEntry): AppStoreMarketEntry {
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

export function applyMockPluginOp(instanceId: string, pluginName: string, action: AppPluginAction) {
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

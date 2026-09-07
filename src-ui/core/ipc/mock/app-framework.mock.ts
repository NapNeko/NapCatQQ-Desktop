// 浏览器预览模式下的应用端（Karin 等）假数据。
// 真 IPC 实装在 core/services/app-framework.service.ts。

import type {
    AppFrameworkManifest,
    AppInstance,
    AppInstanceWebUi,
    CreateAppInstanceRequest,
    OneBotLinkPlan,
} from '../types';
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
    },
    {
        id: 'n9b8c7d6',
        framework_id: 'nonebot2',
        display_name: 'NoneBot2 · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/nonebot2/n9b8c7d6',
        port: 8080,
        state: 'stopped',
        installed_version: '2.4.2',
        created_at_ms: Date.now() - 7_200_000,
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
                req.host_id === 'local'
                    ? `D:/NapCatQQ/apps/${req.framework_id}/${id}`
                    : `/home/ubuntu/ncd/apps/${req.framework_id}/${id}`,
            port: req.port ?? manifest?.default_port ?? 7777,
            state: 'not_installed',
            created_at_ms: Date.now(),
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
};

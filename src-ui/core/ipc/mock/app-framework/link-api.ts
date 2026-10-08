// 对接预览与落笔：预览回包按各框架真机写入形状准备；
// applyLink / unlink 在改实例状态的同时同步写各框架的对接配置。
import type { AppInstance, OneBotLinkPlan } from '../../types';
import { withMockDelay } from '../bootstrap.mock';
import { syncKarinLinkToken, syncMaiBotLink, syncYunzaiLink } from '../app-config.mock';
import { publish, require } from './state';
import { syncKoishiLink } from './koishi';

export const linkApi = {
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
                        summary:
                            '启用适配器 / napcat_server 指向这条连接（host、port、token=<token>）',
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
                    inst.framework_id === 'astrbot'
                        ? '/ws'
                        : inst.framework_id === 'koishi'
                          ? '/onebot/ncd'
                          : '/onebot/v11/ws'
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
                    ? [
                          {
                              path: 'koishi.yml',
                              summary: `adapter-onebot:ncd-link（selfId=${botId}，ws-reverse /onebot/ncd）`,
                          },
                      ]
                    : [
                          {
                              path: '.env',
                              summary: `HTTP_PORT=${inst.port} / WS_SERVER_AUTH_KEY=mock****`,
                          },
                          {
                              path: '@karinjs/config/adapter.json',
                              summary: '开启 onebot.ws_server.enable',
                          },
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
};

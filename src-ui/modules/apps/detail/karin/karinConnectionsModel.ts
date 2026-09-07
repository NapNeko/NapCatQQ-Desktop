// Karin 连接 Tab 的行模型：配置里单例 + 列表摊平成和 Bot 一样的卡片。

import { newOneBotHttpServer, newOneBotWsClient } from '../../../../core/domain/apps/karinConfig';
import type { KarinInstanceConfig } from '../../../../core/ipc/types';

export type KarinConnKind = 'webui' | 'reverseWs' | 'forwardWs' | 'onebotHttp' | 'console';

export type KarinConnRow =
    | { key: string; kind: 'webui'; title: string; enable: boolean; summary: string; removable: false }
    | { key: string; kind: 'reverseWs'; title: string; enable: boolean; summary: string; removable: false; linked: boolean }
    | { key: string; kind: 'forwardWs'; idx: number; title: string; enable: boolean; summary: string; removable: true }
    | { key: string; kind: 'onebotHttp'; idx: number; title: string; enable: boolean; summary: string; removable: true }
    | { key: string; kind: 'console'; title: string; enable: boolean; summary: string; removable: false; localOnly: boolean };

export const KIND_BADGE: Record<KarinConnKind, string> = {
    webui: 'WebUI',
    reverseWs: 'WS-Server',
    forwardWs: 'WS-Client',
    onebotHttp: 'HTTP',
    console: 'Console',
};

export const ADDABLE: ReadonlyArray<{ kind: 'forwardWs' | 'onebotHttp'; title: string }> = [
    { kind: 'forwardWs', title: '正向 WS' },
    { kind: 'onebotHttp', title: 'OneBot HTTP' },
];

export function listKarinConnections(config: KarinInstanceConfig, linked: boolean): KarinConnRow[] {
    const { env, adapter } = config;
    const rows: KarinConnRow[] = [
        {
            key: 'webui',
            kind: 'webui',
            title: 'WebUI',
            enable: env.http_enable,
            summary: `http://${env.http_host || '0.0.0.0'}:${env.http_port}`,
            removable: false,
        },
        {
            key: 'reverseWs',
            kind: 'reverseWs',
            title: '反向 WS',
            enable: adapter.onebot.ws_server.enable,
            summary: [
                env.ws_server_auth_key.trim() ? '已设鉴权' : '无鉴权',
                `超时 ${adapter.onebot.ws_server.timeout}s`,
            ].join(' · '),
            removable: false,
            linked,
        },
    ];
    adapter.onebot.ws_client.forEach((row, idx) => {
        rows.push({
            key: `forwardWs-${idx}`,
            kind: 'forwardWs',
            idx,
            title: row.url.trim() || '正向 WS',
            enable: row.enable,
            summary: row.token.trim() ? `${row.url} · token` : row.url,
            removable: true,
        });
    });
    adapter.onebot.http_server.forEach((row, idx) => {
        rows.push({
            key: `onebotHttp-${idx}`,
            kind: 'onebotHttp',
            idx,
            title: row.url.trim() || 'OneBot HTTP',
            enable: row.enable,
            summary: [row.url, row.self_id.trim() ? `self_id ${row.self_id}` : null]
                .filter(Boolean)
                .join(' · '),
            removable: true,
        });
    });
    rows.push({
        key: 'console',
        kind: 'console',
        title: '控制台',
        enable: true,
        summary: adapter.console.isLocal
            ? '仅本机'
            : adapter.console.host.trim() || '未限制来源',
        removable: false,
        localOnly: adapter.console.isLocal,
    });
    return rows;
}

export function blankForwardWs() {
    return { ...newOneBotWsClient(), enable: true };
}

export function blankOneBotHttp() {
    return { ...newOneBotHttpServer(), enable: true };
}

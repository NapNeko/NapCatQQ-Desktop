// 表单与服务端配置的纯对齐逻辑，从配置页壳挪出：
// applyServerAppLinks 只把 ncd-app:* 客户端跟服务端对齐，其余未保存字段不动；
// normalizeLoadedConfig 补齐磁盘旧数据缺省。

import { replaceAppLinkClients } from '../../../../core/domain/bot/connections';
import { defaultStatusCommandConfig } from '../../../../core/domain/bot/config-defaults';
import { normalizeRuntimeTargetFromDisk } from '../../../../core/domain/bot/runtime-target';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';

export function applyServerAppLinks(draft: BotConfig, server: BotConfig): BotConfig {
    const nextClients = replaceAppLinkClients(
        draft.connect.websocketClients,
        server.connect.websocketClients,
    );
    if (
        nextClients.length === draft.connect.websocketClients.length &&
        nextClients.every(
            (c, i) => JSON.stringify(c) === JSON.stringify(draft.connect.websocketClients[i]),
        )
    ) {
        return draft;
    }
    return { ...draft, connect: { ...draft.connect, websocketClients: nextClients } };
}

export function normalizeLoadedConfig(c: BotConfig): BotConfig {
    let next = c;
    if (c.bot.backend_type === 'snowluma' && !c.statusCommand) {
        next = { ...c, statusCommand: defaultStatusCommandConfig() };
    }
    const rt = normalizeRuntimeTargetFromDisk(next.bot.runtime_target);
    if (rt !== next.bot.runtime_target) {
        next = { ...next, bot: { ...next.bot, runtime_target: rt } };
    }
    return next;
}

export function countConnections(c: BotConfig): number {
    return (
        c.connect.httpServers.length +
        c.connect.httpSseServers.length +
        c.connect.httpClients.length +
        c.connect.websocketServers.length +
        c.connect.websocketClients.length
    );
}

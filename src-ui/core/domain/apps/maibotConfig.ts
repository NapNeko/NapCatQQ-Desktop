// MaiBot 窄配置的前端校验 / 上手判定。规则与后端 `maibot::config::validate` 对齐。
// Desktop 只管端口和聊天名单；模型、人格、插件都在 MaiBot 自己的 WebUI 里配。

import type { AppConfigIssue, MaiBotChatFilter, MaiBotInstanceConfig } from '../../ipc/types';

export function maibotDefaultChat(): MaiBotChatFilter {
    return {
        enable_chat_list_filter: true,
        group_list_type: 'whitelist',
        group_list: [],
        private_list_type: 'whitelist',
        private_list: [],
        ban_user_id: [],
    };
}

export function maibotDefaultConfig(webuiPort: number): MaiBotInstanceConfig {
    return {
        webui_port: webuiPort,
        legacy_ws_port: webuiPort + 1,
        webui_token: '',
        adapter: {
            enabled: false,
            napcat_host: '127.0.0.1',
            napcat_port: 3001,
            has_token: false,
            chat: maibotDefaultChat(),
        },
    };
}

/** 上游默认就是这样：白名单开着、群聊私聊名单都空，适配器把所有消息丢掉，麦麦一句都不回 */
export function maibotChatDropsEverything(chat: MaiBotChatFilter): boolean {
    return (
        chat.enable_chat_list_filter
        && chat.group_list_type === 'whitelist'
        && chat.group_list.length === 0
        && chat.private_list_type === 'whitelist'
        && chat.private_list.length === 0
    );
}

/** 概览里「回复范围」那一格的说法 */
export function maibotChatScope(chat: MaiBotChatFilter): string {
    if (!chat.enable_chat_list_filter) return '所有群聊和私聊';
    const side = (mode: MaiBotChatFilter['group_list_type'], list: string[], what: string) => {
        if (mode === 'whitelist') return list.length ? `${list.length} 个${what}` : `不回${what}`;
        return list.length ? `${what}（除 ${list.length} 个）` : `所有${what}`;
    };
    return `${side(chat.group_list_type, chat.group_list, '群')}，${side(chat.private_list_type, chat.private_list, '私聊')}`;
}

const DIGITS = /^\d+$/;

export function validateMaiBotConfig(cfg: MaiBotInstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    if (!Number.isInteger(cfg.webui_port) || cfg.webui_port < 1) {
        out.push({ path: 'webui_port', message: '端口不能为 0' });
    }
    if (!Number.isInteger(cfg.legacy_ws_port) || cfg.legacy_ws_port < 1) {
        out.push({ path: 'legacy_ws_port', message: '端口不能为 0' });
    }
    if (cfg.webui_port > 0 && cfg.webui_port === cfg.legacy_ws_port) {
        out.push({ path: 'legacy_ws_port', message: '不能和 WebUI 用同一个端口' });
    }
    const chat = cfg.adapter?.chat;
    if (chat) {
        const lists: [string, string[], string][] = [
            ['adapter/chat/group_list', chat.group_list, '群号'],
            ['adapter/chat/private_list', chat.private_list, 'QQ 号'],
            ['adapter/chat/ban_user_id', chat.ban_user_id, 'QQ 号'],
        ];
        for (const [path, list, what] of lists) {
            const bad = list.find((s) => !DIGITS.test(s.trim()));
            if (bad !== undefined) out.push({ path, message: `${what}只能是数字：${bad}` });
        }
    }
    return out;
}

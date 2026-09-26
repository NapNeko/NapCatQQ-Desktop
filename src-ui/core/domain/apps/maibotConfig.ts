// MaiBot 类型化配置的前端校验 / 上手判定。规则与后端 `maibot::config::validate` 对齐，
// 路径前缀同后端：`bot/…` 是 bot_config，`models/…` 是 model_config，`adapter/…` 是适配器名单。

import type { AppConfigIssue, MaiBotChatFilter, MaiBotInstanceConfig } from '../../ipc/types';
// 由 Rust 的 export_bindings_maibot_defaults 导出：和 IPC 上的序列化逐字一致
import maibotDefaults from './maibotDefaults.json';

type MaiBotFiles = Pick<MaiBotInstanceConfig, 'bot' | 'models'>;

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

/** 上游默认的两份主配置（model_config 是首启写出来的那份，带一组 DeepSeek） */
export function maibotDefaultFiles(): MaiBotFiles {
    // JSON 推不出字面量联合类型，这里按生成的 TS 类型认
    return structuredClone(maibotDefaults) as unknown as MaiBotFiles;
}

export function maibotDefaultConfig(webuiPort: number): MaiBotInstanceConfig {
    const files = maibotDefaultFiles();
    files.bot.webui.port = webuiPort;
    files.bot.maim_message.ws_server_port = webuiPort + 1;
    return {
        ...files,
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

const validPort = (p: number) => Number.isInteger(p) && p >= 1 && p <= 65535;

export function validateMaiBotConfig(cfg: MaiBotInstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    const webui = cfg.bot.webui.port;
    const legacy = cfg.bot.maim_message.ws_server_port;
    if (!validPort(webui)) out.push({ path: 'bot/webui/port', message: '要在 1 到 65535 之间' });
    if (!validPort(legacy)) {
        out.push({ path: 'bot/maim_message/ws_server_port', message: '要在 1 到 65535 之间' });
    } else if (webui === legacy) {
        out.push({ path: 'bot/maim_message/ws_server_port', message: '不能和 WebUI 用同一个端口' });
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

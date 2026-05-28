// Bot 配置保存动作的展示派生函数。
//
// 后端 BotManager.upsert_bot_config_with_overrides 在同 backend 同 flavor
// 的运行中分支会把热推送结果用 BotStateChanged 的 reason 字段表达：
//
//   - config_hot_reloaded         热推送成功，配置已生效
//   - config_saved_pending_login  QQ 还没扫码，等下次登录后启动生效
//   - config_saved_pending_reload 网络/鉴权/其它推送失败，等下次重启生效
//   - config_updated              非运行中的兜底文案
//   - config_hot_reload           backend 切换走的重启路径
//
// 这里把 reason 翻成 InfoBar 用的 tone + title + content 三元组，让所有
// 调用方都用同一份文案。

export type SaveTone = 'success' | 'info' | 'warning';

export interface SaveDescriptor {
    tone: SaveTone;
    title: string;
    content: string;
}

/// 把后端 reason 翻成展示文案。`reason` 为空时回退到通用「配置已保存」。
/// `botId` 用于 content 中拼接，便于多 bot 时一眼看出是哪个。
export function describeSaveResult(reason: string | null, botId: string): SaveDescriptor {
    switch (reason) {
        case 'config_hot_reloaded':
            return {
                tone: 'success',
                title: '配置已保存',
                content: `Bot ${botId} 已热推送，配置即时生效`,
            };
        case 'config_saved_pending_login':
            return {
                tone: 'warning',
                title: '配置已保存',
                content: `Bot ${botId} 当前未登录，本次配置将在下次登录启动后生效`,
            };
        case 'config_saved_pending_reload':
            return {
                tone: 'warning',
                title: '配置已保存',
                content: `Bot ${botId} 热推送失败，重启 Bot 后生效`,
            };
        case 'config_hot_reload':
            return {
                tone: 'success',
                title: '配置已保存',
                content: `Bot ${botId} 后端切换已重启`,
            };
        case 'config_updated':
        case null:
        default:
            return {
                tone: 'success',
                title: '配置已保存',
                content: `Bot ${botId} 的配置已更新，下次启动生效`,
            };
    }
}

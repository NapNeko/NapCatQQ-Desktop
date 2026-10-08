// Webhook 通道展示辅助：id 生成 / 显示名 / 摘要。
// 通道列表行与删除确认 Dialog 共用，抽出来避免两处各写一份。

import type { WebhookChannelDraft } from '../../../../core/domain/settings/offline-notify-defaults';

export function newChannelId(existing: WebhookChannelDraft[]): string {
    const stamp = Date.now().toString(36);
    let n = existing.length + 1;
    let id = `channel-${n}-${stamp}`;
    while (existing.some((c) => c.id === id)) {
        n += 1;
        id = `channel-${n}-${stamp}`;
    }
    return id;
}

export function channelDisplayName(ch: WebhookChannelDraft): string {
    const n = ch.name.trim();
    if (n) return n;
    if (ch.url.trim()) {
        try {
            return new URL(ch.url).hostname || ch.id;
        } catch {
            return ch.url.trim().slice(0, 28) || ch.id;
        }
    }
    return '未命名通道';
}

export function channelSummary(ch: WebhookChannelDraft): string {
    if (ch.url.trim()) {
        try {
            return new URL(ch.url).host;
        } catch {
            return ch.url.trim();
        }
    }
    return '未填写地址';
}

// SnowLuma 不发 group_upload 通知，开着的群文件列表只能靠消息流里最新一条文件消息感知更新。
import type { Message } from './model';

export function latestFileMessageKey(messages: readonly Message[]): string {
    for (let i = messages.length - 1; i >= 0; i--)
        if (messages[i].status === 'sent' && messages[i].segments.some((s) => s.type === 'file'))
            return messages[i].key;
    return '';
}

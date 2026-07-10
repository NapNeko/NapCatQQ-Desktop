// 离线通知默认模板：独立模块，避免 settings.service ↔ settings.mock 循环依赖。

/** Server酱风格默认 Webhook body */
export const DEFAULT_WEBHOOK_BODY = `{
  "title": "账号状态通知：{event}",
  "desp": "您的账号状态发生了改变。\\n\\n**昵称**：{nickname}\\n**QQ号**：{uin}\\n**当前状态**：{event}\\n**时间**：{time}"
}`;

/** 钉钉机器人 markdown */
export const DINGTALK_WEBHOOK_BODY = `{
  "msgtype": "markdown",
  "markdown": {
    "title": "账号状态通知：{event}",
    "text": "### 账号状态通知：{event}\\n\\n- **昵称**：{nickname}\\n- **QQ号**：{uin}\\n- **状态**：{event}\\n- **时间**：{time}"
  }
}`;

/** 飞书 text */
export const FEISHU_WEBHOOK_BODY = `{
  "msg_type": "text",
  "content": {
    "text": "账号状态通知：{event}\\n昵称：{nickname}\\nQQ：{uin}\\n时间：{time}"
  }
}`;

/** Discord inbound webhook */
export const DISCORD_WEBHOOK_BODY = `{
  "content": null,
  "embeds": [
    {
      "title": "账号状态通知：{event}",
      "description": "**昵称**：{nickname}\\n**QQ**：{uin}\\n**状态**：{event}\\n**时间**：{time}",
      "color": 15158332
    }
  ]
}`;

/** Bark */
export const BARK_WEBHOOK_BODY = `{
  "title": "账号状态通知：{event}",
  "body": "昵称：{nickname}\\nQQ：{uin}\\n状态：{event}\\n时间：{time}",
  "group": "NapCatQQ Desktop"
}`;

export const DEFAULT_ONEBOT_MESSAGE =
    '【掉线通知】{nickname}({uin}) 状态={event} 时间={time}';

/** 前端 Webhook 通道草稿形状（对齐 OfflineWebhookChannel，字段用 camelCase 便于 UI） */
export interface WebhookChannelDraft {
    id: string;
    name: string;
    enabled: boolean;
    url: string;
    secret: string;
    bodyTemplate: string;
    method: string;
}

export type WebhookPresetId =
    | 'serverchan'
    | 'dingtalk'
    | 'feishu'
    | 'discord'
    | 'bark';

export const WEBHOOK_PRESETS: Array<{
    id: WebhookPresetId;
    label: string;
    body: string;
}> = [
        { id: 'serverchan', label: 'Server酱', body: DEFAULT_WEBHOOK_BODY },
        { id: 'dingtalk', label: '钉钉', body: DINGTALK_WEBHOOK_BODY },
        { id: 'feishu', label: '飞书', body: FEISHU_WEBHOOK_BODY },
        { id: 'discord', label: 'Discord', body: DISCORD_WEBHOOK_BODY },
        { id: 'bark', label: 'Bark', body: BARK_WEBHOOK_BODY },
    ];

export function createBlankWebhookChannel(
    id: string,
    name = '',
): WebhookChannelDraft {
    return {
        id,
        name,
        enabled: true,
        url: '',
        secret: '',
        bodyTemplate: DEFAULT_WEBHOOK_BODY,
        method: 'POST',
    };
}

/** 从后端 channels / 扁平字段合成可编辑列表 */
export function coerceWebhookChannels(input: {
    channels?: Array<{
        id?: string;
        name?: string;
        enabled?: boolean;
        url?: string;
        secret?: string;
        body_template?: string;
        method?: string;
    }>;
    url?: string;
    secret?: string;
    bodyTemplate?: string;
    method?: string;
}): WebhookChannelDraft[] {
    const raw = input.channels ?? [];
    if (raw.length > 0) {
        return raw.map((ch, i) => ({
            id: (ch.id ?? '').trim() || `channel-${i + 1}`,
            name: ch.name ?? '',
            enabled: ch.enabled ?? true,
            url: ch.url ?? '',
            secret: ch.secret ?? '',
            bodyTemplate:
                (ch.body_template ?? '').trim() || DEFAULT_WEBHOOK_BODY,
            method: ((ch.method ?? 'POST').trim() || 'POST').toUpperCase(),
        }));
    }
    if ((input.url ?? '').trim()) {
        return [
            {
                id: 'legacy',
                name: '默认',
                enabled: true,
                url: input.url ?? '',
                secret: input.secret ?? '',
                bodyTemplate:
                    (input.bodyTemplate ?? '').trim() || DEFAULT_WEBHOOK_BODY,
                method: ((input.method ?? 'POST').trim() || 'POST').toUpperCase(),
            },
        ];
    }
    return [];
}

export function webhookChannelsEqual(
    a: WebhookChannelDraft[],
    b: WebhookChannelDraft[],
): boolean {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i += 1) {
        const x = a[i];
        const y = b[i];
        if (
            x.id !== y.id ||
            x.name !== y.name ||
            x.enabled !== y.enabled ||
            x.url !== y.url ||
            x.secret !== y.secret ||
            x.bodyTemplate !== y.bodyTemplate ||
            x.method !== y.method
        ) {
            return false;
        }
    }
    return true;
}
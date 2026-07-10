// Webhook 消息可视化模型。
// 用户不直接改 JSON：按服务类型填标题/正文等字段，再序列化成 body_template。
// 持久化仍是字符串模板（占位符 {nickname}/{uin}/{event}/{time}），与后端 render_template 对齐。

import {
    BARK_WEBHOOK_BODY,
    DEFAULT_WEBHOOK_BODY,
    DINGTALK_WEBHOOK_BODY,
    DISCORD_WEBHOOK_BODY,
    FEISHU_WEBHOOK_BODY,
    type WebhookPresetId,
} from './offline-notify-defaults';

export type WebhookServiceKind = WebhookPresetId | 'custom';

export interface WebhookVisualFields {
    title: string;
    body: string;
    /** Bark 分组；其它服务忽略 */
    group: string;
}

export const TEMPLATE_VARS: ReadonlyArray<{
    key: 'nickname' | 'uin' | 'event' | 'time';
    label: string;
    sample: string;
}> = [
        { key: 'nickname', label: '昵称', sample: '测试 Bot' },
        { key: 'uin', label: 'QQ', sample: '10001' },
        { key: 'event', label: '状态', sample: '掉线' },
        { key: 'time', label: '时间', sample: '2026-01-01 12:00:00' },
    ];

const DISCORD_EMBED_COLOR = 15158332;
const DEFAULT_BARK_GROUP = 'NapCatQQ Desktop';

export function emptyVisualFields(): WebhookVisualFields {
    return {
        title: '账号状态通知：{event}',
        body: '昵称：{nickname}\nQQ：{uin}\n状态：{event}\n时间：{time}',
        group: DEFAULT_BARK_GROUP,
    };
}

/** 从 body 模板猜测服务类型（结构优先，精确匹配次之） */
export function detectWebhookService(bodyTemplate: string): WebhookServiceKind {
    const raw = bodyTemplate.trim();
    if (!raw) return 'serverchan';

    const presets: Array<{ id: WebhookPresetId; body: string }> = [
        { id: 'serverchan', body: DEFAULT_WEBHOOK_BODY },
        { id: 'dingtalk', body: DINGTALK_WEBHOOK_BODY },
        { id: 'feishu', body: FEISHU_WEBHOOK_BODY },
        { id: 'discord', body: DISCORD_WEBHOOK_BODY },
        { id: 'bark', body: BARK_WEBHOOK_BODY },
    ];
    for (const p of presets) {
        if (normalizeJsonText(p.body) === normalizeJsonText(raw)) return p.id;
    }

    let value: unknown;
    try {
        value = JSON.parse(raw);
    } catch {
        return 'custom';
    }
    if (!value || typeof value !== 'object' || Array.isArray(value)) return 'custom';
    const obj = value as Record<string, unknown>;

    if (obj.msgtype === 'markdown' && isRecord(obj.markdown)) return 'dingtalk';
    if (obj.msg_type === 'text' && isRecord(obj.content)) return 'feishu';
    if (Array.isArray(obj.embeds)) return 'discord';
    if (
        typeof obj.title === 'string' &&
        typeof obj.body === 'string' &&
        (typeof obj.group === 'string' || obj.group === undefined)
    ) {
        return 'bark';
    }
    if (typeof obj.title === 'string' && typeof obj.desp === 'string') return 'serverchan';
    return 'custom';
}

export function parseVisualFields(
    bodyTemplate: string,
    kind: WebhookServiceKind = detectWebhookService(bodyTemplate),
): WebhookVisualFields | null {
    if (kind === 'custom') return null;
    const fallback = fieldsFromPresetBody(kind);
    const raw = bodyTemplate.trim();
    if (!raw) return fallback;

    let value: unknown;
    try {
        value = JSON.parse(raw);
    } catch {
        return fallback;
    }
    if (!isRecord(value)) return fallback;

    switch (kind) {
        case 'serverchan':
            return {
                title: str(value.title, fallback.title),
                body: str(value.desp, fallback.body),
                group: fallback.group,
            };
        case 'dingtalk': {
            const md = isRecord(value.markdown) ? value.markdown : {};
            return {
                title: str(md.title, fallback.title),
                body: str(md.text, fallback.body),
                group: fallback.group,
            };
        }
        case 'feishu': {
            const content = isRecord(value.content) ? value.content : {};
            return {
                title: fallback.title,
                body: str(content.text, fallback.body),
                group: fallback.group,
            };
        }
        case 'discord': {
            const embeds = Array.isArray(value.embeds) ? value.embeds : [];
            const first = isRecord(embeds[0]) ? embeds[0] : {};
            return {
                title: str(first.title, fallback.title),
                body: str(first.description, fallback.body),
                group: fallback.group,
            };
        }
        case 'bark':
            return {
                title: str(value.title, fallback.title),
                body: str(value.body, fallback.body),
                group: str(value.group, fallback.group) || DEFAULT_BARK_GROUP,
            };
        default:
            return fallback;
    }
}

export function serializeVisualFields(
    kind: WebhookServiceKind,
    fields: WebhookVisualFields,
): string {
    if (kind === 'custom') {
        throw new Error('custom kind has no visual serializer');
    }
    const title = fields.title;
    const body = fields.body;
    const group = fields.group.trim() || DEFAULT_BARK_GROUP;

    let payload: unknown;
    switch (kind) {
        case 'serverchan':
            payload = { title, desp: body };
            break;
        case 'dingtalk':
            payload = {
                msgtype: 'markdown',
                markdown: { title, text: body },
            };
            break;
        case 'feishu':
            payload = {
                msg_type: 'text',
                content: { text: body },
            };
            break;
        case 'discord':
            payload = {
                content: null,
                embeds: [
                    {
                        title,
                        description: body,
                        color: DISCORD_EMBED_COLOR,
                    },
                ],
            };
            break;
        case 'bark':
            payload = { title, body, group };
            break;
    }
    return `${JSON.stringify(payload, null, 2)}\n`;
}

export function fieldsFromPresetBody(kind: WebhookPresetId): WebhookVisualFields {
    // 直接给默认字段，避免 parse ↔ preset 循环
    switch (kind) {
        case 'serverchan':
            return {
                title: '账号状态通知：{event}',
                body: '您的账号状态发生了改变。\n\n**昵称**：{nickname}\n**QQ号**：{uin}\n**当前状态**：{event}\n**时间**：{time}',
                group: DEFAULT_BARK_GROUP,
            };
        case 'dingtalk':
            return {
                title: '账号状态通知：{event}',
                body: '### 账号状态通知：{event}\n\n- **昵称**：{nickname}\n- **QQ号**：{uin}\n- **状态**：{event}\n- **时间**：{time}',
                group: DEFAULT_BARK_GROUP,
            };
        case 'feishu':
            return {
                title: '账号状态通知：{event}',
                body: '账号状态通知：{event}\n昵称：{nickname}\nQQ：{uin}\n时间：{time}',
                group: DEFAULT_BARK_GROUP,
            };
        case 'discord':
            return {
                title: '账号状态通知：{event}',
                body: '**昵称**：{nickname}\n**QQ**：{uin}\n**状态**：{event}\n**时间**：{time}',
                group: DEFAULT_BARK_GROUP,
            };
        case 'bark':
            return {
                title: '账号状态通知：{event}',
                body: '昵称：{nickname}\nQQ：{uin}\n状态：{event}\n时间：{time}',
                group: DEFAULT_BARK_GROUP,
            };
    }
}

export function serviceFieldMeta(kind: WebhookServiceKind): {
    showTitle: boolean;
    showBody: boolean;
    showGroup: boolean;
    titleLabel: string;
    bodyLabel: string;
    bodyHint?: string;
} {
    switch (kind) {
        case 'serverchan':
            return {
                showTitle: true,
                showBody: true,
                showGroup: false,
                titleLabel: '标题',
                bodyLabel: '详情 (desp)',
                bodyHint: '支持 Markdown 换行',
            };
        case 'dingtalk':
            return {
                showTitle: true,
                showBody: true,
                showGroup: false,
                titleLabel: '标题',
                bodyLabel: 'Markdown 正文',
                bodyHint: '钉钉 markdown.text',
            };
        case 'feishu':
            return {
                showTitle: false,
                showBody: true,
                showGroup: false,
                titleLabel: '标题',
                bodyLabel: '文本内容',
            };
        case 'discord':
            return {
                showTitle: true,
                showBody: true,
                showGroup: false,
                titleLabel: 'Embed 标题',
                bodyLabel: 'Embed 描述',
            };
        case 'bark':
            return {
                showTitle: true,
                showBody: true,
                showGroup: true,
                titleLabel: '标题',
                bodyLabel: '正文',
            };
        case 'custom':
            return {
                showTitle: false,
                showBody: false,
                showGroup: false,
                titleLabel: '标题',
                bodyLabel: '正文',
            };
    }
}

/** 预览：把占位符换成样例文案（不保证合法 JSON，仅给人读） */
export function previewTemplate(template: string): string {
    let out = template;
    for (const v of TEMPLATE_VARS) {
        out = out.split(`{${v.key}}`).join(v.sample);
    }
    return out;
}

function isRecord(v: unknown): v is Record<string, unknown> {
    return !!v && typeof v === 'object' && !Array.isArray(v);
}

function str(v: unknown, fallback: string): string {
    return typeof v === 'string' ? v : fallback;
}

function normalizeJsonText(text: string): string {
    try {
        return JSON.stringify(JSON.parse(text));
    } catch {
        return text.trim();
    }
}

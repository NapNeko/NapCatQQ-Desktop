import { describe, expect, it } from 'vitest';
import {
    BARK_WEBHOOK_BODY,
    DEFAULT_ONEBOT_MESSAGE,
    DEFAULT_WEBHOOK_BODY,
    DINGTALK_WEBHOOK_BODY,
    DISCORD_WEBHOOK_BODY,
    FEISHU_WEBHOOK_BODY,
    WEBHOOK_PRESETS,
    coerceWebhookChannels,
    createBlankWebhookChannel,
    webhookChannelsEqual,
    type WebhookChannelDraft,
} from './offline-notify-defaults';

const PLACEHOLDERS = ['{event}', '{nickname}', '{uin}', '{time}'];

describe('默认模板', () => {
    it('五个 preset 模板都能解析为 JSON 且带齐占位符', () => {
        expect(WEBHOOK_PRESETS.map((p) => p.id)).toEqual([
            'serverchan',
            'dingtalk',
            'feishu',
            'discord',
            'bark',
        ]);
        for (const preset of WEBHOOK_PRESETS) {
            expect(() => JSON.parse(preset.body)).not.toThrow();
            for (const ph of PLACEHOLDERS) {
                expect(preset.body).toContain(ph);
            }
        }
    });

    it('preset body 与具名常量一一对应', () => {
        const byId = new Map(WEBHOOK_PRESETS.map((p) => [p.id, p.body]));
        expect(byId.get('serverchan')).toBe(DEFAULT_WEBHOOK_BODY);
        expect(byId.get('dingtalk')).toBe(DINGTALK_WEBHOOK_BODY);
        expect(byId.get('feishu')).toBe(FEISHU_WEBHOOK_BODY);
        expect(byId.get('discord')).toBe(DISCORD_WEBHOOK_BODY);
        expect(byId.get('bark')).toBe(BARK_WEBHOOK_BODY);
    });

    it('钉钉是 markdown 消息、飞书是 text 消息', () => {
        expect(JSON.parse(DINGTALK_WEBHOOK_BODY)).toMatchObject({ msgtype: 'markdown' });
        expect(JSON.parse(FEISHU_WEBHOOK_BODY)).toMatchObject({ msg_type: 'text' });
    });

    it('Server酱 的 desp 里换行是 JSON 转义后真实存在的 \n', () => {
        const parsed = JSON.parse(DEFAULT_WEBHOOK_BODY) as { desp: string };
        expect(parsed.desp).toContain('\n\n');
    });

    it('OneBot 默认消息带齐四个占位符', () => {
        for (const ph of PLACEHOLDERS) {
            expect(DEFAULT_ONEBOT_MESSAGE).toContain(ph);
        }
    });
});

describe('createBlankWebhookChannel', () => {
    it('新通道默认启用、POST、Server酱 模板', () => {
        const draft = createBlankWebhookChannel('ch-1');
        expect(draft).toEqual({
            id: 'ch-1',
            name: '',
            enabled: true,
            url: '',
            secret: '',
            bodyTemplate: DEFAULT_WEBHOOK_BODY,
            method: 'POST',
        });
    });

    it('可带显示名', () => {
        expect(createBlankWebhookChannel('ch-2', '掉线推钉钉').name).toBe('掉线推钉钉');
    });
});

describe('coerceWebhookChannels', () => {
    it('后端 channels 存在时逐条映射，method 归一大写', () => {
        const out = coerceWebhookChannels({
            channels: [
                {
                    id: 'a',
                    name: '主通道',
                    enabled: false,
                    url: 'https://example.test/hook',
                    secret: 's3cret',
                    body_template: BARK_WEBHOOK_BODY,
                    method: 'post',
                },
                { id: 'b', method: ' get ' },
            ],
            url: 'https://legacy.test/hook',
        });
        expect(out).toHaveLength(2);
        expect(out[0]).toEqual({
            id: 'a',
            name: '主通道',
            enabled: false,
            url: 'https://example.test/hook',
            secret: 's3cret',
            bodyTemplate: BARK_WEBHOOK_BODY,
            method: 'POST',
        });
        expect(out[1]).toMatchObject({ id: 'b', enabled: true, method: 'GET' });
    });

    it('缺 id / body_template 时补序号 id 与默认模板', () => {
        const out = coerceWebhookChannels({
            channels: [{ url: 'https://x.test' }, { url: 'https://y.test' }],
        });
        expect(out[0]!.id).toBe('channel-1');
        expect(out[1]!.id).toBe('channel-2');
        expect(out[0]!.bodyTemplate).toBe(DEFAULT_WEBHOOK_BODY);
    });

    it('id 只有空白也算缺，退回序号', () => {
        expect(
            coerceWebhookChannels({ channels: [{ id: '   ', url: 'https://x.test' }] })[0]!.id,
        ).toBe('channel-1');
    });

    it('无 channels 但有旧版扁平 url 时合成 legacy 单通道', () => {
        const out = coerceWebhookChannels({
            url: 'https://old.test/hook',
            secret: 'k',
            method: 'put',
        });
        expect(out).toHaveLength(1);
        expect(out[0]).toMatchObject({
            id: 'legacy',
            name: '默认',
            enabled: true,
            url: 'https://old.test/hook',
            secret: 'k',
            bodyTemplate: DEFAULT_WEBHOOK_BODY,
            method: 'PUT',
        });
    });

    it('channels 与扁平 url 都为空（或只有空白）返回空列表', () => {
        expect(coerceWebhookChannels({})).toEqual([]);
        expect(coerceWebhookChannels({ channels: [], url: '   ' })).toEqual([]);
    });

    it('channels 优先于扁平字段，不重复合成 legacy', () => {
        const out = coerceWebhookChannels({
            channels: [{ id: 'a', url: 'https://a.test' }],
            url: 'https://legacy.test',
        });
        expect(out.map((c) => c.id)).toEqual(['a']);
    });

    it('输出形状与草稿往返：coerce 结果原样再比较等于自身', () => {
        const draft = createBlankWebhookChannel('r1', '往返');
        const back = coerceWebhookChannels({
            channels: [
                {
                    id: draft.id,
                    name: draft.name,
                    enabled: draft.enabled,
                    url: draft.url,
                    secret: draft.secret,
                    body_template: draft.bodyTemplate,
                    method: draft.method,
                },
            ],
        });
        expect(webhookChannelsEqual(back, [draft])).toBe(true);
    });
});

describe('webhookChannelsEqual', () => {
    const base = (): WebhookChannelDraft => ({
        id: 'a',
        name: '主',
        enabled: true,
        url: 'https://a.test',
        secret: 's',
        bodyTemplate: DEFAULT_WEBHOOK_BODY,
        method: 'POST',
    });

    it('内容相同即相等，不要求同一引用', () => {
        expect(webhookChannelsEqual([base()], [base()])).toBe(true);
        expect(webhookChannelsEqual([], [])).toBe(true);
    });

    it('长度或顺序不同不等', () => {
        expect(webhookChannelsEqual([base()], [])).toBe(false);
        const other = { ...base(), id: 'b' };
        expect(webhookChannelsEqual([base(), other], [other, base()])).toBe(false);
    });

    it('逐字段敏感性：任一字段变化都算改动', () => {
        const mutations: Array<Partial<WebhookChannelDraft>> = [
            { name: '改名' },
            { enabled: false },
            { url: 'https://b.test' },
            { secret: 'rotated' },
            { bodyTemplate: BARK_WEBHOOK_BODY },
            { method: 'GET' },
        ];
        for (const m of mutations) {
            expect(webhookChannelsEqual([base()], [{ ...base(), ...m }])).toBe(false);
        }
    });
});

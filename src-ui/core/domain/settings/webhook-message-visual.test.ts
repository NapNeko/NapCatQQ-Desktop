import { describe, expect, it } from 'vitest';
import {
    detectWebhookService,
    fieldsFromPresetBody,
    parseVisualFields,
    serializeVisualFields,
} from './webhook-message-visual';
import {
    BARK_WEBHOOK_BODY,
    DEFAULT_WEBHOOK_BODY,
    DINGTALK_WEBHOOK_BODY,
    DISCORD_WEBHOOK_BODY,
    FEISHU_WEBHOOK_BODY,
} from './offline-notify-defaults';

describe('webhook-message-visual', () => {
    it('detects known preset bodies', () => {
        expect(detectWebhookService(DEFAULT_WEBHOOK_BODY)).toBe('serverchan');
        expect(detectWebhookService(DINGTALK_WEBHOOK_BODY)).toBe('dingtalk');
        expect(detectWebhookService(FEISHU_WEBHOOK_BODY)).toBe('feishu');
        expect(detectWebhookService(DISCORD_WEBHOOK_BODY)).toBe('discord');
        expect(detectWebhookService(BARK_WEBHOOK_BODY)).toBe('bark');
        expect(detectWebhookService('{ "foo": 1 }')).toBe('custom');
    });

    it('round-trips visual fields for each service', () => {
        const kinds = [
            'serverchan',
            'dingtalk',
            'feishu',
            'discord',
            'bark',
        ] as const;
        for (const kind of kinds) {
            const fields = fieldsFromPresetBody(kind);
            fields.title = 'T {event}';
            fields.body = 'B {nickname}';
            if (kind === 'bark') fields.group = 'G';
            const json = serializeVisualFields(kind, fields);
            expect(detectWebhookService(json)).toBe(kind);
            const parsed = parseVisualFields(json, kind);
            expect(parsed?.title).toBe(fields.title);
            expect(parsed?.body).toBe(fields.body);
            if (kind === 'bark') expect(parsed?.group).toBe('G');
        }
    });

    it('preserves content when remapping service shell', () => {
        const base = fieldsFromPresetBody('serverchan');
        base.title = '掉线：{event}';
        base.body = 'QQ={uin}';
        const dingtalk = serializeVisualFields('dingtalk', base);
        const parsed = parseVisualFields(dingtalk, 'dingtalk');
        expect(parsed?.title).toBe('掉线：{event}');
        expect(parsed?.body).toBe('QQ={uin}');
        expect(detectWebhookService(dingtalk)).toBe('dingtalk');
    });
});

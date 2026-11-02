import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DebugCallRequest } from '../generated/debug/DebugCallRequest';
import type { DebugEventBatch } from '../generated/debug/DebugEventBatch';
import { onebotDebugMock as mock, resetOnebotDebugMock } from './onebot-debug.mock';

async function settle<T>(pending: Promise<T>): Promise<T> {
    await vi.advanceTimersByTimeAsync(1000);
    return pending;
}

beforeEach(() => {
    vi.useFakeTimers();
    resetOnebotDebugMock();
});

afterEach(() => {
    resetOnebotDebugMock();
    vi.useRealTimers();
});

describe('调试台浏览器预览', () => {
    it('可选目标的接口目录能打开，支持发送消息', async () => {
        const targets = await settle(mock.targets());
        for (const backend of ['napcat', 'snowluma'] as const) {
            const target = targets.find((item) => item.backend === backend && item.running);
            expect(target).toBeDefined();
            const catalog = await settle(mock.catalog(target!.bot_id, backend));
            expect(catalog.actions).toEqual(
                expect.arrayContaining([
                    expect.objectContaining({ name: 'send_group_msg', supported: true }),
                ]),
            );
        }
    });

    it('发送消息会回显到订阅，关闭预览后释放定时器', async () => {
        const botId = 'mock-bot-nc-remote';
        const batches: DebugEventBatch[] = [];
        const subscription = await settle(
            mock.subscribe(botId, { kind: 'auto' }, (batch) => batches.push(batch)),
        );
        try {
            const request: DebugCallRequest = {
                request_id: 'preview-send',
                bot_id: botId,
                channel: { kind: 'auto' },
                action: 'send_group_msg',
                params: { group_id: '100001', message: '预览消息' },
                timeout_ms: null,
                origin: 'composer',
            };
            const response = await settle(mock.call(request));
            expect(response.request_id).toBe(request.request_id);
            if (response.result.kind !== 'ok') throw new Error('预览消息未返回结果');
            expect(response.result.outcome.ok).toBe(true);
            const { message_id } = response.result.outcome.data as { message_id: number };
            expect(message_id).toBeDefined();
            expect(batches.flatMap((batch) => batch.events)).toEqual(
                expect.arrayContaining([
                    expect.objectContaining({
                        body: expect.objectContaining({
                            kind: 'ob11',
                            payload: expect.objectContaining({
                                post_type: 'message_sent',
                                message_id,
                                message: [{ type: 'text', data: { text: '预览消息' } }],
                            }),
                        }),
                    }),
                ]),
            );
        } finally {
            await mock.unsubscribe(subscription.subscription_id);
        }
        expect(vi.getTimerCount()).toBe(0);
    });
});

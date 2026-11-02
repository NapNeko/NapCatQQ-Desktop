import { describe, expect, it } from 'vitest';
import { chatMock } from './chat.mock';
import type { DebugCallRequest } from '../generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../generated/debug/DebugCallResponse';
import type { DebugEventBatch } from '../generated/debug/DebugEventBatch';

function request(action: string, params: unknown): DebugCallRequest {
    return {
        bot_id: 'mock-chat-actions',
        request_id: crypto.randomUUID(),
        action,
        params,
        channel: { kind: 'auto' },
        origin: 'picker',
        timeout_ms: 30_000,
    };
}
function data(response: DebugCallResponse): Record<string, unknown> {
    if (response.result.kind !== 'ok') throw new Error('预览调用失败');
    return response.result.outcome.data as Record<string, unknown>;
}

describe('chat action previews', () => {
    it('keeps forwarded message content readable and emits a recall for its original conversation', async () => {
        const batches: DebugEventBatch[] = [];
        const subscription = await chatMock.subscribe('mock-chat-actions', (batch) =>
            batches.push(batch),
        );
        try {
            const original = data(
                await chatMock.call(
                    request('send_group_msg', {
                        group_id: '20001',
                        message: [{ type: 'text', data: { text: '转发预览内容' } }],
                    }),
                ),
            );
            const forwarded = data(
                await chatMock.call(
                    request('send_private_forward_msg', {
                        user_id: '10021',
                        messages: [{ type: 'node', data: { id: original.message_id } }],
                    }),
                ),
            );
            const detail = data(
                await chatMock.call(request('get_forward_msg', { id: forwarded.forward_id })),
            );
            expect(detail.messages).toEqual(
                expect.arrayContaining([
                    expect.objectContaining({
                        message: [{ type: 'text', data: { text: '转发预览内容' } }],
                    }),
                ]),
            );
            const other = await chatMock.call({
                ...request('get_forward_msg', { id: forwarded.forward_id }),
                bot_id: 'another-preview-account',
            });
            expect(other.result.kind === 'ok' && other.result.outcome.ok).toBe(false);
            await chatMock.call(request('delete_msg', { message_id: original.message_id }));
            const event = batches.at(-1)?.events[0];
            expect(event?.body).toEqual({
                kind: 'ob11',
                payload: {
                    post_type: 'notice',
                    notice_type: 'group_recall',
                    group_id: '20001',
                    message_id: original.message_id,
                },
            });
        } finally {
            await chatMock.unsubscribe(subscription.subscription_id);
        }
    });
});

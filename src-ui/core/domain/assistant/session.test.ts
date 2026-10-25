import { describe, expect, it } from 'vitest';
import type { AssistantEvent, AssistantSession } from './types';
import { applyAssistantEvent, assistantTimeline } from './session';

function snapshot(sequence = 0): AssistantSession {
    return {
        sequence,
        info: {
            id: 'session-1',
            provider: 'dsh',
            botId: '10001',
            botName: '测试 Bot',
            title: '诊断',
            state: 'ready',
            createdAtMs: 100,
            updatedAtMs: 100,
            error: null,
            capabilities: {
                mcpHttp: true,
                loadSession: true,
                resumeSession: true,
                images: false,
                elicitation: true,
            },
        },
        messages: [],
        tools: [],
        permissions: [],
        questions: [],
    };
}

function event(sequence: number, update: AssistantEvent['update']): AssistantEvent {
    return { v: 1, sessionId: 'session-1', sequence, update };
}

describe('诊断会话增量', () => {
    it('流式分段更新同一条回复，保留用户消息与快照原对象', () => {
        const initial = snapshot();
        initial.messages.push({ id: 'user-1', role: 'user', text: '收不到消息', createdAtMs: 101 });
        const partial = applyAssistantEvent(
            initial,
            event(1, {
                kind: 'message',
                message: {
                    id: 'reply-1',
                    role: 'assistant',
                    text: '正在检查',
                    createdAtMs: 102,
                },
            }),
        );
        const complete = applyAssistantEvent(
            partial,
            event(2, {
                kind: 'message',
                message: {
                    id: 'reply-1',
                    role: 'assistant',
                    text: '正在检查连接，已找到原因。',
                    createdAtMs: 102,
                },
            }),
        );
        expect(complete.messages.map((message) => message.text)).toEqual([
            '收不到消息',
            '正在检查连接，已找到原因。',
        ]);
        expect(initial.messages).toHaveLength(1);
        expect(complete.sequence).toBe(2);
    });

    it('快照包含较新回复时，旧分段与相同水位均不能把回复回滚', () => {
        const current = snapshot(12);
        current.messages.push({
            id: 'reply',
            role: 'assistant',
            text: '完整结论',
            createdAtMs: 101,
        });
        for (const sequence of [9, 12]) {
            const result = applyAssistantEvent(
                current,
                event(sequence, {
                    kind: 'message',
                    message: {
                        id: 'reply',
                        role: 'assistant',
                        text: '完整',
                        createdAtMs: 101,
                    },
                }),
            );
            expect(result).toBe(current);
            expect(result.messages[0]?.text).toBe('完整结论');
        }
        const unrelated = { ...event(13, { kind: 'removed' }), sessionId: 'other-session' };
        expect(applyAssistantEvent(current, unrelated)).toBe(current);
    });

    it('审批完成只移除对应交互，保留另一条追问', () => {
        const current = snapshot(2);
        current.permissions.push({
            id: 'approval',
            source: 'ncdOperation',
            title: '重启测试 Bot',
            details: 'restart_bot',
            options: [],
            createdAtMs: 102,
        });
        current.questions.push({
            id: 'question',
            title: '什么时候开始？',
            fields: [],
            createdAtMs: 103,
        });
        const result = applyAssistantEvent(
            current,
            event(3, { kind: 'interactionResolved', id: 'approval' }),
        );
        expect(result.permissions).toEqual([]);
        expect(result.questions.map((question) => question.id)).toEqual(['question']);
        expect(current.permissions).toHaveLength(1);
    });

    it('消息、工具和待处理交互按发生时间排列', () => {
        const current = snapshot();
        current.messages.push({ id: 'reply', role: 'assistant', text: '结果', createdAtMs: 400 });
        current.tools.push({
            id: 'tool',
            title: 'diagnose_bot',
            status: 'completed',
            details: '',
            createdAtMs: 200,
        });
        current.permissions.push({
            id: 'permission',
            source: 'ncdOperation',
            title: '启动',
            details: '',
            options: [],
            createdAtMs: 300,
        });
        current.questions.push({ id: 'question', title: '现象', fields: [], createdAtMs: 100 });
        expect(assistantTimeline(current).map((item) => item.kind)).toEqual([
            'question',
            'tool',
            'permission',
            'message',
        ]);
        expect(current.messages[0]?.id).toBe('reply');
    });
});

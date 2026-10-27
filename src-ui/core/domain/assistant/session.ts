// 会话快照和增量事件的纯合并逻辑。

import type {
    AssistantEvent,
    AssistantMessage,
    AssistantPermissionRequest,
    AssistantQuestion,
    AssistantSession,
    AssistantSessionState,
    AssistantToolCall,
} from './types';

export const assistantStateLabel: Record<AssistantSessionState, string> = {
    starting: '连接中',
    ready: '就绪',
    running: '思考中',
    waitingForInput: '等待回复',
    disconnected: '已断开',
    closed: '已关闭',
    failed: '连接失败',
};

export const assistantStarterPrompts = [
    '帮我检查当前运行状态',
    '为什么收不到消息？',
    '最近有没有报错？',
] as const;

export function isAssistantBusy(state: AssistantSessionState): boolean {
    return state === 'starting' || state === 'running' || state === 'waitingForInput';
}

function upsert<T extends { id: string }>(items: T[], item: T): T[] {
    const index = items.findIndex((entry) => entry.id === item.id);
    if (index < 0) return [...items, item];
    return items.map((entry, i) => (i === index ? item : entry));
}

export function applyAssistantEvent(
    session: AssistantSession,
    event: AssistantEvent,
): AssistantSession {
    if (event.sessionId !== session.info.id || event.sequence <= session.sequence) return session;
    const next = { ...session, sequence: event.sequence };
    const update = event.update;
    switch (update.kind) {
        case 'session':
            return { ...next, info: update.info };
        case 'message':
            return { ...next, messages: upsert(session.messages, update.message) };
        case 'tool':
            return { ...next, tools: upsert(session.tools, update.tool) };
        case 'permission':
            return { ...next, permissions: upsert(session.permissions, update.request) };
        case 'question':
            return { ...next, questions: upsert(session.questions, update.question) };
        case 'interactionResolved':
            return {
                ...next,
                permissions: session.permissions.filter((item) => item.id !== update.id),
                questions: session.questions.filter((item) => item.id !== update.id),
            };
        case 'removed':
            return next;
    }
}

export type AssistantTimelineItem =
    | { kind: 'message'; value: AssistantMessage }
    | { kind: 'tool'; value: AssistantToolCall }
    | { kind: 'permission'; value: AssistantPermissionRequest }
    | { kind: 'question'; value: AssistantQuestion };

export type AssistantTimelineBlock =
    | Exclude<AssistantTimelineItem, { kind: 'tool' }>
    | { kind: 'tools'; value: AssistantToolCall[] };

// 连续的工具调用折成一组，避免一轮诊断刷出一长串工具卡片。
export function groupAssistantTimeline(items: AssistantTimelineItem[]): AssistantTimelineBlock[] {
    const blocks: AssistantTimelineBlock[] = [];
    for (const item of items) {
        if (item.kind !== 'tool') {
            blocks.push(item);
            continue;
        }
        const last = blocks[blocks.length - 1];
        if (last?.kind === 'tools') last.value.push(item.value);
        else blocks.push({ kind: 'tools', value: [item.value] });
    }
    return blocks;
}

export function assistantTimeline(session: AssistantSession): AssistantTimelineItem[] {
    return [
        ...session.messages.map((value): AssistantTimelineItem => ({ kind: 'message', value })),
        ...session.tools.map((value): AssistantTimelineItem => ({ kind: 'tool', value })),
        ...session.permissions.map((value): AssistantTimelineItem => ({
            kind: 'permission',
            value,
        })),
        ...session.questions.map((value): AssistantTimelineItem => ({ kind: 'question', value })),
    ].sort(
        (a, b) => a.value.createdAtMs - b.value.createdAtMs || a.value.id.localeCompare(b.value.id),
    );
}

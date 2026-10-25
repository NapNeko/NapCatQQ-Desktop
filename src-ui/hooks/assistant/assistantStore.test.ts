// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AssistantEvent, AssistantSession } from '../../core/domain/assistant/types';

const service = vi.hoisted(() => ({
    subscribe: vi.fn(),
    listSessions: vi.fn(),
    getSession: vi.fn(),
    createSession: vi.fn(),
    sendMessage: vi.fn(),
    cancel: vi.fn(),
    close: vi.fn(),
    delete: vi.fn(),
    respondPermission: vi.fn(),
    respondQuestion: vi.fn(),
}));
vi.mock('../../core/services/assistant.service', () => ({ assistantService: service }));

function session(id = 'session-1', sequence = 0): AssistantSession {
    return {
        sequence,
        info: {
            id,
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

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((done) => {
        resolve = done;
    });
    return { promise, resolve };
}

let receive: (event: AssistantEvent) => void;
let store: (typeof import('./assistantStore'))['assistantStore'];
const emit = (sequence: number, update: AssistantEvent['update']) =>
    receive({ v: 1, sessionId: 'session-1', sequence, update });

beforeEach(async () => {
    vi.resetModules();
    Object.values(service).forEach((mock) => mock.mockReset());
    service.subscribe.mockImplementation(async (listener) => {
        receive = listener;
        return () => {};
    });
    service.listSessions.mockResolvedValue({ sessions: [session().info], notices: [] });
    service.getSession.mockImplementation(async (id: string) => session(id));
    service.sendMessage.mockResolvedValue(undefined);
    service.cancel.mockResolvedValue(undefined);
    service.close.mockResolvedValue(undefined);
    service.respondPermission.mockResolvedValue(undefined);
    service.respondQuestion.mockResolvedValue(undefined);
    vi.spyOn(window, 'addEventListener').mockImplementation(() => {});
    store = (await import('./assistantStore')).assistantStore;
    await store.ensure();
});
afterEach(() => {
    vi.restoreAllMocks();
});

describe('诊断会话快照补偿', () => {
    it('拉取期间更晚的流式事件会重放到旧快照上', async () => {
        const request = deferred<AssistantSession>();
        service.getSession.mockReturnValueOnce(request.promise);
        store.select('session-1');
        emit(1, { kind: 'session', info: { ...session().info, state: 'running' } });
        emit(2, {
            kind: 'message',
            message: { id: 'reply', role: 'assistant', text: '新分段', createdAtMs: 101 },
        });
        request.resolve(session());
        await vi.waitFor(() => expect(store.getSnapshot().sessions['session-1']?.sequence).toBe(2));
        expect(store.getSnapshot().sessions['session-1']?.info.state).toBe('running');
        expect(store.getSnapshot().sessions['session-1']?.messages[0]?.text).toBe('新分段');
    });

    it('快照水位比缓冲区新时，迟到分段不能回滚回复或会话状态', async () => {
        const request = deferred<AssistantSession>();
        service.getSession.mockReturnValueOnce(request.promise);
        store.select('session-1');
        emit(1, {
            kind: 'message',
            message: { id: 'reply', role: 'assistant', text: '半', createdAtMs: 101 },
        });
        const newer = session('session-1', 5);
        newer.messages.push({ id: 'reply', role: 'assistant', text: '完整结论', createdAtMs: 101 });
        request.resolve(newer);
        await vi.waitFor(() => expect(store.getSnapshot().sessions['session-1']?.sequence).toBe(5));
        emit(3, {
            kind: 'message',
            message: { id: 'reply', role: 'assistant', text: '半句话', createdAtMs: 101 },
        });
        emit(4, { kind: 'session', info: { ...newer.info, state: 'running' } });
        expect(store.getSnapshot().sessions['session-1']?.messages[0]?.text).toBe('完整结论');
        expect(store.getSnapshot().sessions['session-1']?.info.state).toBe('ready');
        expect(service.getSession).toHaveBeenCalledTimes(1);
    });

    it('序号跳跃会补读丢失的工具与结论，相同序号不会覆盖新值', async () => {
        store.select('session-1');
        await vi.waitFor(() => expect(store.getSnapshot().sessions['session-1']).toBeDefined());
        const complete = session('session-1', 4);
        complete.tools.push({
            id: 'tool',
            title: 'diagnose_bot',
            status: 'completed',
            details: '连接正常',
            createdAtMs: 101,
        });
        complete.messages.push({ id: 'reply', role: 'assistant', text: '结论', createdAtMs: 102 });
        service.getSession.mockResolvedValueOnce(complete);
        emit(4, { kind: 'message', message: complete.messages[0]! });
        await vi.waitFor(() =>
            expect(store.getSnapshot().sessions['session-1']?.tools).toHaveLength(1),
        );
        emit(4, { kind: 'message', message: { ...complete.messages[0]!, text: '重复旧值' } });
        expect(store.getSnapshot().sessions['session-1']?.messages[0]?.text).toBe('结论');
        expect(service.getSession).toHaveBeenCalledTimes(2);
    });
});

describe('诊断会话操作', () => {
    it('切换会话不会串草稿，发送失败保留原文，成功只清除发送会话草稿', async () => {
        store.draft('session-1', '原始问题');
        store.draft('session-2', '另一条问题');
        store.select('session-2');
        service.sendMessage.mockRejectedValueOnce('Agent 连接失败');
        expect(await store.send('session-1', '原始问题')).toBe(false);
        expect(store.getSnapshot().drafts).toMatchObject({
            'session-1': '原始问题',
            'session-2': '另一条问题',
        });
        expect(store.getSnapshot().error).toBe('Agent 连接失败');
        expect(await store.send('session-2', '另一条问题')).toBe(true);
        expect(store.getSnapshot().drafts['session-2']).toBe('');
        expect(store.getSnapshot().drafts['session-1']).toBe('原始问题');
    });

    it('审批与追问绑定原会话和请求，失败保留错误并允许重试', async () => {
        service.respondPermission.mockRejectedValueOnce(new Error('审批已过期'));
        expect(await store.permission('session-1', 'approval-1', 'allow_once')).toBe(false);
        expect(service.respondPermission).toHaveBeenCalledWith(
            'session-1',
            'approval-1',
            'allow_once',
        );
        expect(store.getSnapshot().error).toBe('审批已过期');
        expect(store.getSnapshot().pending).not.toContain('approval-1');
        const answer = { cancelled: false, values: { when: '更新之后' } };
        expect(await store.question('session-1', 'question-1', answer)).toBe(true);
        expect(service.respondQuestion).toHaveBeenCalledWith('session-1', 'question-1', answer);
        expect(service.createSession).not.toHaveBeenCalled();
    });

    it('停止与关闭调用各自入口，停止后保留草稿和会话记录', async () => {
        store.draft('session-1', '接着问');
        expect(await store.cancel('session-1')).toBe(true);
        expect(service.cancel).toHaveBeenCalledWith('session-1');
        expect(service.close).not.toHaveBeenCalled();
        expect(store.getSnapshot().drafts['session-1']).toBe('接着问');
        service.getSession.mockResolvedValueOnce({
            ...session(),
            info: { ...session().info, state: 'closed' },
        });
        expect(await store.close('session-1')).toBe(true);
        expect(service.close).toHaveBeenCalledWith('session-1');
        expect(store.getSnapshot().sessions['session-1']?.info.state).toBe('closed');
        expect(store.getSnapshot().sessions['session-1']?.messages).toEqual([]);
    });
});

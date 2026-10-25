// 浏览器专用界面样例，不启动 agent 或操作 Bot。

import type {
    AssistantEvent,
    AssistantProvider,
    AssistantProviderStatus,
    AssistantModelStatus,
    AssistantQuestionAnswer,
    AssistantSession,
    AssistantSettings,
    AssistantUpdate,
} from '../../domain/assistant/types';
import { modelOrigin, validateBuiltinSettings } from '../../domain/assistant/model';

const labels: Record<AssistantProvider, string> = {
    builtin: '内置助手',
    dsh: 'DSH',
    qwen: 'Qwen Code',
    kimi: 'Kimi Code',
    miniMax: 'MiniMax MCode',
    codeBuddy: 'CodeBuddy Code',
    qoder: 'Qoder CLI',
    trae: 'Trae CLI',
    custom: '自定义 ACP Agent',
};
let config: AssistantSettings = {
    defaultProvider: 'builtin',
    builtin: { baseUrl: '', model: '', maxSteps: 6, maxOutputTokens: 2048, requestTimeoutSecs: 90 },
    providers: (Object.keys(labels) as AssistantProvider[])
        .filter((provider) => provider !== 'builtin')
        .map((provider) => ({
            provider,
            enabled: ['dsh', 'qwen', 'kimi', 'miniMax'].includes(provider),
            executablePath: null,
            extraArgs: [],
            allowNativeTools: false,
        })),
};
let keyOrigin: string | null = null;
const sessions = new Map<string, AssistantSession>();
const sequences = new Map<string, number>();
const generations = new Map<string, number>();
const subscribers = new Set<(event: AssistantEvent) => void>();
const copy = <T>(value: T): T => structuredClone(value);
const id = () => crypto.randomUUID();

function sessionFor(sessionId: string): AssistantSession {
    const session = sessions.get(sessionId);
    if (!session) throw new Error('会话已不存在');
    return session;
}

function emit(sessionId: string, update: AssistantUpdate): void {
    const sequence = (sequences.get(sessionId) ?? 0) + 1;
    sequences.set(sessionId, sequence);
    const session = sessions.get(sessionId);
    if (session) session.sequence = sequence;
    const event: AssistantEvent = { v: 1, sessionId, sequence, update: copy(update) };
    subscribers.forEach((listener) => listener(event));
}

function state(sessionId: string, next: AssistantSession['info']['state']): void {
    const session = sessionFor(sessionId);
    session.info = { ...session.info, state: next, updatedAtMs: Date.now() };
    emit(sessionId, { kind: 'session', info: session.info });
}

function message(sessionId: string, role: 'user' | 'assistant' | 'system', text: string): void {
    const session = sessionFor(sessionId);
    const item = { id: id(), role, text, createdAtMs: Date.now() };
    session.messages.push(item);
    emit(sessionId, { kind: 'message', message: item });
}

function invalidate(sessionId: string): number {
    const next = (generations.get(sessionId) ?? 0) + 1;
    generations.set(sessionId, next);
    return next;
}

function finish(sessionId: string, text: string, generation: number): void {
    const session = sessionFor(sessionId);
    const item = { id: id(), role: 'assistant' as const, text: '', createdAtMs: Date.now() };
    session.messages.push(item);
    let offset = 0;
    const tick = () => {
        if (!sessions.has(sessionId) || generations.get(sessionId) !== generation) return;
        offset += 12;
        item.text = text.slice(0, offset);
        emit(sessionId, { kind: 'message', message: item });
        if (offset < text.length) setTimeout(tick, 45);
        else state(sessionId, 'ready');
    };
    tick();
}

export const assistantMock = {
    async modelStatus(): Promise<AssistantModelStatus> {
        const hasApiKey = keyOrigin !== null && keyOrigin === modelOrigin(config.builtin.baseUrl);
        return {
            hasApiKey,
            configured:
                hasApiKey &&
                !!config.builtin.model.trim() &&
                Object.keys(validateBuiltinSettings(config.builtin)).length === 0,
            error: null,
        };
    },
    async setApiKey(apiKey: string): Promise<AssistantModelStatus> {
        const origin = modelOrigin(config.builtin.baseUrl);
        if (!origin || validateBuiltinSettings(config.builtin).baseUrl || !apiKey.trim())
            throw new Error('请先保存服务地址，再输入 API Key。');
        for (const session of sessions.values()) {
            if (session.info.provider === 'builtin' && session.info.state !== 'closed') {
                await this.cancel(session.info.id);
            }
        }
        // 预览只记录配置状态，不保存输入的 Key，也不访问网络。
        keyOrigin = origin;
        return this.modelStatus();
    },
    async clearApiKey(): Promise<AssistantModelStatus> {
        for (const session of sessions.values()) {
            if (session.info.provider === 'builtin' && session.info.state !== 'closed') {
                await this.cancel(session.info.id);
            }
        }
        if (keyOrigin === modelOrigin(config.builtin.baseUrl)) keyOrigin = null;
        return this.modelStatus();
    },
    async settings(): Promise<AssistantSettings> {
        return copy(config);
    },
    async saveSettings(next: AssistantSettings): Promise<void> {
        const errors = validateBuiltinSettings(next.builtin);
        const error = Object.values(errors)[0];
        if (error) throw new Error(error);
        config = copy(next);
    },
    async providers(): Promise<AssistantProviderStatus[]> {
        return [
            {
                provider: 'builtin',
                label: labels.builtin,
                enabled: true,
                installed: true,
                executablePath: null,
                version: null,
                error: null,
                documentationUrl: '',
            },
            ...config.providers.map((provider) => ({
                provider: provider.provider,
                label: labels[provider.provider],
                enabled: provider.enabled,
                installed: ['dsh', 'qwen'].includes(provider.provider),
                executablePath: provider.executablePath,
                version: ['dsh', 'qwen'].includes(provider.provider) ? '界面样例' : null,
                error: null,
                documentationUrl: 'https://agentclientprotocol.com/get-started/agents',
            })),
        ];
    },
    async listSessions() {
        return { sessions: copy([...sessions.values()].map((entry) => entry.info)), notices: [] };
    },
    async getSession(sessionId: string): Promise<AssistantSession> {
        return copy(sessionFor(sessionId));
    },
    async createSession(provider: AssistantProvider, botId: string): Promise<AssistantSession> {
        if (provider === 'builtin' && !(await this.modelStatus()).configured)
            throw new Error('请先配置模型与 API Key。');
        const now = Date.now();
        const session: AssistantSession = {
            sequence: 0,
            info: {
                id: id(),
                provider,
                botId,
                botName: `Bot-${botId.slice(-2)}`,
                title: '新诊断',
                state: 'ready',
                capabilities: {
                    mcpHttp: true,
                    loadSession: true,
                    resumeSession: true,
                    images: false,
                    elicitation: true,
                },
                createdAtMs: now,
                updatedAtMs: now,
                error: null,
            },
            messages: [],
            tools: [],
            permissions: [],
            questions: [],
        };
        sessions.set(session.info.id, session);
        emit(session.info.id, { kind: 'session', info: session.info });
        return copy(session);
    },
    async sendMessage(sessionId: string, text: string): Promise<void> {
        const session = sessionFor(sessionId);
        if (!['ready', 'disconnected', 'failed'].includes(session.info.state))
            throw new Error('请先完成当前交互');
        const generation = invalidate(sessionId);
        message(sessionId, 'user', text);
        session.info.title = text.slice(0, 24);
        state(sessionId, 'running');
        setTimeout(() => {
            if (!sessions.has(sessionId) || generations.get(sessionId) !== generation) return;
            const tool = {
                id: id(),
                title: 'diagnose_bot',
                status: 'completed',
                details: '界面样例：已读取所选 Bot 的脱敏运行摘要。',
                createdAtMs: Date.now(),
            };
            session.tools.push(tool);
            emit(sessionId, { kind: 'tool', tool });
            if (text.includes('重启')) {
                const request = {
                    id: id(),
                    source: 'ncdOperation' as const,
                    title: `重启 ${session.info.botName}`,
                    details: `目标 Bot：${session.info.botId}\n操作：restart_bot\n界面预览不会执行此操作。`,
                    options: [
                        { id: 'allow_once', label: '仅允许这一次', kind: 'allow_once' },
                        { id: 'reject', label: '拒绝', kind: 'reject_once' },
                    ],
                    createdAtMs: Date.now(),
                };
                session.permissions.push(request);
                emit(sessionId, { kind: 'permission', request });
                state(sessionId, 'waitingForInput');
            } else if (text.includes('追问') || text.includes('确认')) {
                const question = {
                    id: id(),
                    title: '问题从什么时候开始出现？',
                    createdAtMs: Date.now(),
                    fields: [
                        {
                            name: 'when',
                            title: '出现时间',
                            description: null,
                            kind: 'string',
                            required: true,
                            options: ['刚刚开始', '更新之后', '一直如此'],
                        },
                    ],
                };
                session.questions.push(question);
                emit(sessionId, { kind: 'question', question });
                state(sessionId, 'waitingForInput');
            } else {
                finish(
                    sessionId,
                    '**这是界面预览，没有调用真实模型。**\n\n诊断助手会围绕所选 Bot 检查运行状态、连接和最近的缓存日志，再给出处理建议。\n\n你可以输入“重启”体验操作审批，输入“追问”体验补充信息。',
                    generation,
                );
            }
        }, 450);
    },
    async cancel(sessionId: string): Promise<void> {
        invalidate(sessionId);
        const session = sessionFor(sessionId);
        [...session.permissions, ...session.questions].forEach((item) =>
            emit(sessionId, { kind: 'interactionResolved', id: item.id }),
        );
        session.permissions = [];
        session.questions = [];
        state(sessionId, 'disconnected');
        message(sessionId, 'system', '已停止本轮界面演示。');
    },
    async close(sessionId: string): Promise<void> {
        await this.cancel(sessionId);
        state(sessionId, 'closed');
    },
    async delete(sessionId: string): Promise<void> {
        invalidate(sessionId);
        sessions.delete(sessionId);
        emit(sessionId, { kind: 'removed' });
    },
    async respondPermission(
        sessionId: string,
        requestId: string,
        optionId: string | null,
    ): Promise<void> {
        const session = sessionFor(sessionId);
        if (!session.permissions.some((request) => request.id === requestId))
            throw new Error('此审批已结束');
        session.permissions = session.permissions.filter((request) => request.id !== requestId);
        emit(sessionId, { kind: 'interactionResolved', id: requestId });
        state(sessionId, 'running');
        finish(
            sessionId,
            optionId === 'allow_once'
                ? '界面样例：你已批准本次操作。预览不会真的重启 Bot。'
                : '已拒绝这次操作。可以继续描述问题。',
            generations.get(sessionId) ?? 0,
        );
    },
    async respondQuestion(
        sessionId: string,
        requestId: string,
        answer: AssistantQuestionAnswer,
    ): Promise<void> {
        const session = sessionFor(sessionId);
        if (!session.questions.some((question) => question.id === requestId))
            throw new Error('此追问已结束');
        session.questions = session.questions.filter((question) => question.id !== requestId);
        emit(sessionId, { kind: 'interactionResolved', id: requestId });
        state(sessionId, 'running');
        finish(
            sessionId,
            answer.cancelled
                ? '已跳过这次追问。'
                : `界面样例：已收到补充信息：${Object.values(answer.values).join('，')}。`,
            generations.get(sessionId) ?? 0,
        );
    },
    async subscribe(listener: (event: AssistantEvent) => void): Promise<() => void> {
        subscribers.add(listener);
        return () => {
            subscribers.delete(listener);
        };
    },
};

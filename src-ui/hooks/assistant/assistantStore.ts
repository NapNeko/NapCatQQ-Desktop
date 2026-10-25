// 会话与草稿在路由外存活；快照补读期间重放增量，避免覆盖正在流入的回复。

import { assistantService } from '../../core/services/assistant.service';
import { applyAssistantEvent } from '../../core/domain/assistant/session';
import type {
    AssistantEvent,
    AssistantProvider,
    AssistantQuestionAnswer,
    AssistantSession,
    AssistantSessionInfo,
} from '../../core/domain/assistant/types';
import { createStore } from '../utils/createStore';

interface AssistantState {
    infos: AssistantSessionInfo[];
    sessions: Record<string, AssistantSession>;
    selectedId: string | null;
    drafts: Record<string, string>;
    pending: string[];
    loading: boolean;
    error: string | null;
    notices: string[];
}

const store = createStore<AssistantState>({
    infos: [],
    sessions: {},
    selectedId: null,
    drafts: {},
    pending: [],
    loading: false,
    error: null,
    notices: [],
});
const sequences = new Map<string, number>();
const refreshing = new Map<string, Promise<void>>();
const replay = new Map<string, AssistantEvent[]>();
const removed = new Set<string>();
let initialized: Promise<void> | null = null;
let subscribed = false;
let focusInstalled = false;
const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));
const patch = (next: Partial<AssistantState>) =>
    store.setState({ ...store.getSnapshot(), ...next });

function putInfo(
    infos: AssistantSessionInfo[],
    info: AssistantSessionInfo,
): AssistantSessionInfo[] {
    return [...infos.filter((entry) => entry.id !== info.id), info].sort(
        (a, b) => b.updatedAtMs - a.updatedAtMs,
    );
}

function removeSession(sessionId: string): void {
    removed.add(sessionId);
    const current = store.getSnapshot();
    const sessions = { ...current.sessions };
    const drafts = { ...current.drafts };
    delete sessions[sessionId];
    delete drafts[sessionId];
    for (const key of Object.keys(drafts)) {
        if (key.startsWith(`question:${sessionId}:`)) delete drafts[key];
    }
    patch({
        sessions,
        drafts,
        infos: current.infos.filter((info) => info.id !== sessionId),
        selectedId: current.selectedId === sessionId ? null : current.selectedId,
    });
}

function receive(event: AssistantEvent): void {
    if (event.v !== 1 || removed.has(event.sessionId)) return;
    const previous = sequences.get(event.sessionId) ?? 0;
    if (event.sequence <= previous) return;
    sequences.set(event.sessionId, event.sequence);
    if (event.update.kind === 'removed') {
        removeSession(event.sessionId);
        return;
    }
    const buffered = replay.get(event.sessionId);
    if (buffered) buffered.push(event);
    const current = store.getSnapshot();
    const session = current.sessions[event.sessionId];
    const info = event.update.kind === 'session' ? event.update.info : null;
    const drafts = event.update.kind === 'interactionResolved' ? { ...current.drafts } : null;
    if (drafts && event.update.kind === 'interactionResolved')
        delete drafts[`question:${event.sessionId}:${event.update.id}`];
    patch({
        ...(info ? { infos: putInfo(current.infos, info) } : {}),
        ...(session
            ? {
                  sessions: {
                      ...current.sessions,
                      [event.sessionId]: applyAssistantEvent(session, event),
                  },
              }
            : {}),
        ...(drafts ? { drafts } : {}),
    });
    if (event.sequence !== previous + 1 || (!session && current.selectedId === event.sessionId)) {
        void refreshSession(event.sessionId).catch((error) => patch({ error: errorText(error) }));
    }
}

async function refreshSession(sessionId: string): Promise<void> {
    const running = refreshing.get(sessionId);
    if (running) return running;
    const events: AssistantEvent[] = [];
    replay.set(sessionId, events);
    const task = (async () => {
        try {
            let session = await assistantService.getSession(sessionId);
            if (removed.has(sessionId)) return;
            for (const event of events) {
                if (event.sequence > session.sequence)
                    session = applyAssistantEvent(session, event);
            }
            sequences.set(sessionId, Math.max(sequences.get(sessionId) ?? 0, session.sequence));
            const current = store.getSnapshot();
            patch({
                sessions: { ...current.sessions, [sessionId]: session },
                infos: putInfo(current.infos, session.info),
            });
        } finally {
            replay.delete(sessionId);
            refreshing.delete(sessionId);
        }
    })();
    refreshing.set(sessionId, task);
    return task;
}

async function refresh(): Promise<void> {
    patch({ loading: true, error: null });
    const knownIds = new Set(store.getSnapshot().infos.map((info) => info.id));
    try {
        const result = await assistantService.listSessions();
        const current = store.getSnapshot();
        const infos = result.sessions
            .filter((info) => !removed.has(info.id))
            .map((info) => {
                const newer = current.infos.find((entry) => entry.id === info.id);
                return newer && newer.updatedAtMs > info.updatedAtMs ? newer : info;
            });
        for (const info of current.infos) {
            if (
                !knownIds.has(info.id) &&
                !infos.some((entry) => entry.id === info.id) &&
                !removed.has(info.id)
            )
                infos.push(info);
        }
        patch({
            infos: infos.sort((a, b) => b.updatedAtMs - a.updatedAtMs),
            notices: result.notices,
        });
        const selected = store.getSnapshot().selectedId;
        if (selected && infos.some((info) => info.id === selected)) await refreshSession(selected);
    } finally {
        patch({ loading: false });
    }
}

async function ensure(): Promise<void> {
    if (initialized) return initialized;
    initialized = (async () => {
        try {
            if (!subscribed) {
                await assistantService.subscribe(receive);
                subscribed = true;
            }
            if (!focusInstalled) {
                window.addEventListener('focus', () => {
                    void refresh().catch((error) => patch({ error: errorText(error) }));
                });
                focusInstalled = true;
            }
            await refresh();
        } catch (error) {
            initialized = null;
            patch({ error: errorText(error) });
        }
    })();
    return initialized;
}

async function action(key: string, run: () => Promise<void>): Promise<boolean> {
    if (store.getSnapshot().pending.includes(key)) return false;
    patch({ pending: [...store.getSnapshot().pending, key], error: null });
    try {
        await run();
        return true;
    } catch (error) {
        patch({ error: errorText(error) });
        return false;
    } finally {
        patch({ pending: store.getSnapshot().pending.filter((item) => item !== key) });
    }
}

export const assistantStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,
    ensure,
    refresh: () =>
        action('refresh', async () => {
            if (!subscribed) {
                initialized = null;
                await ensure();
            } else await refresh();
        }),
    select(sessionId: string | null): void {
        patch({ selectedId: sessionId, error: null });
        if (sessionId)
            void refreshSession(sessionId).catch((error) => patch({ error: errorText(error) }));
    },
    draft(key: string, value: string): void {
        patch({ drafts: { ...store.getSnapshot().drafts, [key]: value } });
    },
    clearError(): void {
        patch({ error: null });
    },
    create: (provider: AssistantProvider, botId: string) =>
        action('create', async () => {
            const session = await assistantService.createSession(provider, botId);
            removed.delete(session.info.id);
            sequences.set(
                session.info.id,
                Math.max(sequences.get(session.info.id) ?? 0, session.sequence),
            );
            const current = store.getSnapshot();
            patch({
                infos: putInfo(current.infos, session.info),
                sessions: { ...current.sessions, [session.info.id]: session },
                selectedId: session.info.id,
            });
            await refreshSession(session.info.id);
        }),
    send: (sessionId: string, text: string) =>
        action(`send:${sessionId}`, async () => {
            await assistantService.sendMessage(sessionId, text);
            assistantStore.draft(sessionId, '');
            await refreshSession(sessionId);
        }),
    cancel: (sessionId: string) =>
        action(`cancel:${sessionId}`, async () => {
            await assistantService.cancel(sessionId);
            await refreshSession(sessionId);
        }),
    close: (sessionId: string) =>
        action(`close:${sessionId}`, async () => {
            await assistantService.close(sessionId);
            await refreshSession(sessionId);
        }),
    delete: (sessionId: string) =>
        action(`delete:${sessionId}`, async () => {
            await assistantService.delete(sessionId);
            removeSession(sessionId);
        }),
    permission: (sessionId: string, requestId: string, optionId: string | null) =>
        action(requestId, async () => {
            await assistantService.respondPermission(sessionId, requestId, optionId);
            await refreshSession(sessionId);
        }),
    question: (sessionId: string, requestId: string, answer: AssistantQuestionAnswer) =>
        action(requestId, async () => {
            await assistantService.respondQuestion(sessionId, requestId, answer);
            await refreshSession(sessionId);
        }),
};

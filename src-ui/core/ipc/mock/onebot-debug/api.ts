// 对外接口：onebotDebugMock 的方法在这里组装，状态与逻辑在同目录的分模块里；
// resetOnebotDebugMock 依次跑调用 / 事件流 / Bot / 收藏各自的 reset（互不依赖，顺序无关）。

import type { BackendType } from '../../generated/domain/BackendType';
import type { DebugActionSpec } from '../../generated/debug/DebugActionSpec';
import type { DebugCallResponse } from '../../generated/debug/DebugCallResponse';
import type { DebugCatalog } from '../../generated/debug/DebugCatalog';
import type { DebugChannelId } from '../../generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../../generated/debug/DebugChannelInfo';
import type { DebugChannels } from '../../generated/debug/DebugChannels';
import type { DebugCollections } from '../../generated/debug/DebugCollections';
import type { DebugEvent } from '../../generated/debug/DebugEvent';
import type { DebugEventBatch } from '../../generated/debug/DebugEventBatch';
import type { DebugHistoryEntry } from '../../generated/debug/DebugHistoryEntry';
import type { DebugHistoryPage } from '../../generated/debug/DebugHistoryPage';
import type { DebugHistoryQuery } from '../../generated/debug/DebugHistoryQuery';
import type { DebugReceiverInfo } from '../../generated/debug/DebugReceiverInfo';
import type { DebugStreamCallRequest } from '../../generated/debug/DebugStreamCallRequest';
import type { DebugStreamProgress } from '../../generated/debug/DebugStreamProgress';
import type { DebugSubscribeResponse } from '../../generated/debug/DebugSubscribeResponse';
import type { DebugTarget } from '../../generated/debug/DebugTarget';
import type { DebugWorkspace } from '../../generated/debug/DebugWorkspace';
import { isTauri } from '../../transport';
import { buildMockCatalog, buildMockSpec, type MockSpecOptions } from './catalog-build';
import {
    BOTS,
    findBot,
    channelDefs,
    channelInfo,
    channelKey,
    pickAuto,
    resolveChannel,
    errorText,
    respond,
    rejectAfterDelay,
    statusOverrides,
    overrideKey,
    currentStatus,
    mockGeneration,
    resetBotsMock,
} from './bots';
import {
    states,
    subscriptions,
    stateOf,
    ensureReceiver,
    startTimers,
    stopTimers,
    catchUp,
    receiverInfo,
    shutdownReceiver,
    flood,
    nextSubscriptionId,
    resetEventMock,
    type Subscriber,
} from './state';
import { memberCache } from './handlers';
import {
    call,
    pending,
    files,
    largeResponses,
    historyStore,
    clearHistoryStore,
    takeStorageNotices,
    pushStorageNotice,
    resetCallMock,
} from './call';
import {
    workspaceStore,
    collectionsStore,
    mergeCollections,
    replaceWorkspace,
    replaceCollections,
    resetCollectionsMock,
} from './collections';
import {
    EVENT_VERSION,
    BACKLOG_CHUNK,
    TEST_CHANNEL_MS,
    LARGE_RESPONSES_KEPT,
    clone,
    isRecord,
} from './consts';

// ---------------------------------------------------------------------------
// 对外接口
// ---------------------------------------------------------------------------

function catalogFlavor(botId: string | null): MockSpecOptions {
    const bot = botId ? findBot(botId) : undefined;
    // 没选 Bot 或 Bot 没在跑：只有内置快照；在线目录里缺的动作由这个 Bot 的老版本决定
    if (!bot || !bot.running) return { source: 'snapshot' };
    return { source: 'live', unsupported: bot.missingActions };
}

export const onebotDebugMock = {
    targets: (): Promise<DebugTarget[]> =>
        respond(
            BOTS.map((b) => ({
                bot_id: b.id,
                name: b.name,
                qq_id: b.qq,
                backend: b.backend,
                host: clone(b.host),
                running: b.running,
                online: b.online,
            })),
        ),

    channels: (botId: string): Promise<DebugChannels> => {
        const bot = findBot(botId);
        if (!bot) return rejectAfterDelay(errorText({ kind: 'bot_not_found' }));
        const infos = channelDefs(bot).map((d) => channelInfo(bot, d));
        return respond({
            bot_id: bot.id,
            channels: infos,
            auto_call: pickAuto(bot, 'call'),
            auto_events: pickAuto(bot, 'events'),
        });
    },

    testChannel: (botId: string, channel: DebugChannelId): Promise<DebugChannelInfo> => {
        const bot = findBot(botId);
        if (!bot) return rejectAfterDelay(errorText({ kind: 'bot_not_found' }));
        const target = channel.kind === 'auto' ? pickAuto(bot, 'call') : channel;
        const def = target
            ? channelDefs(bot).find((d) => channelKey(d.id) === channelKey(target))
            : undefined;
        if (!def) return rejectAfterDelay('这个 Bot 没有这条通道');
        const generation = mockGeneration;
        // 和后端一样测完才记结果：探测进行中拉通道列表，看到的还是旧状态
        return new Promise((resolve) => {
            setTimeout(() => {
                // 测的途中预览被重置过，结果就作废，不往新的状态里写
                if (generation === mockGeneration) {
                    const before = currentStatus(bot, def);
                    // 没测过的 WS 通道测一下就通了；令牌错的 HTTP 服务被上游拒绝
                    let after = before;
                    if (before.kind === 'unknown') after = { kind: 'available' };
                    else if (
                        before.kind === 'tunneled' &&
                        def.id.kind === 'http' &&
                        bot.httpRejectsToken
                    ) {
                        after = { kind: 'auth_failed', status: 401 };
                    }
                    if (after !== before) statusOverrides.set(overrideKey(bot.id, def.id), after);
                }
                resolve(channelInfo(bot, def));
            }, TEST_CHANNEL_MS);
        });
    },

    catalog: (botId: string | null, backend: BackendType): Promise<DebugCatalog> =>
        respond(buildMockCatalog(backend, catalogFlavor(botId))),

    describe: (
        botId: string | null,
        backend: BackendType,
        action: string,
    ): Promise<DebugActionSpec | null> =>
        respond(buildMockSpec(backend, action, catalogFlavor(botId))),

    call,

    /**
     * 假流式调用：先把 4 拍进度推完，再按普通调用收尾；
     * 取消在进入普通调用之后才生效（预览不急这两拍）
     */
    callStream: async (
        request: DebugStreamCallRequest,
        onProgress: (p: DebugStreamProgress) => void,
    ): Promise<DebugCallResponse> => {
        const total = 4 * 1024 * 1024;
        const name = request.local_files[0]?.path.split(/[\\/]/).pop() ?? 'preview.bin';
        for (const ratio of [0.1, 0.4, 0.7, 1]) {
            await respond(undefined);
            onProgress({
                v: 1,
                request_id: request.request_id,
                stage: 'uploading',
                file_name: name,
                done_bytes: Math.round(total * ratio),
                total_bytes: total,
                done_chunks: 0,
                total_chunks: null,
            });
        }
        return call({
            request_id: request.request_id,
            bot_id: request.bot_id,
            channel: request.channel,
            action: request.action,
            params: request.params,
            timeout_ms: request.timeout_ms,
            origin: request.origin,
        });
    },

    pickLocalFile: (): Promise<{ path: string; name: string } | null> =>
        respond({ path: 'C:\\预览\\本机文件.png', name: '本机文件.png' }),

    cancel: (requestId: string): Promise<void> => {
        pending.get(requestId)?.cancel();
        return Promise.resolve();
    },

    /** 只认最近几次被截断的调用；没截断的回包界面里已经是全的，后端也不留全文 */
    saveResponse: (requestId: string, path: string): Promise<void> => {
        const text = largeResponses.get(requestId);
        if (text === undefined) {
            return rejectAfterDelay(
                `没有这次调用的完整回包：只保留最近 ${LARGE_RESPONSES_KEPT} 次被截断的回包`,
            );
        }
        files.set(path, text);
        return respond(undefined);
    },

    subscribe: async (
        botId: string,
        source: DebugChannelId,
        onBatch: (batch: DebugEventBatch) => void,
    ): Promise<DebugSubscribeResponse> => {
        await respond(undefined);
        const bot = findBot(botId);
        if (!bot) throw errorText({ kind: 'bot_not_found' });
        const resolved = resolveChannel(bot, source, 'events');
        if (!resolved.ok) throw errorText(resolved.error);

        const st = stateOf(bot);
        const rc = ensureReceiver(st, resolved.def.id);
        if (rc.subs.size === 0 && rc.pausedAt !== null) catchUp(st, rc.pausedAt, Date.now());

        // 先把缓冲里已有的按 500 条一批补给它，再登记进实时推送：中间不漏也不重
        for (let i = 0; i < st.ring.length; i += BACKLOG_CHUNK) {
            onBatch({
                v: EVENT_VERSION,
                bot_id: bot.id,
                events: st.ring.slice(i, i + BACKLOG_CHUNK),
            });
        }
        const sub: Subscriber = {
            id: `mock-sub-${nextSubscriptionId()}`,
            onBatch,
            queue: [],
            flushTimer: null,
        };
        rc.subs.set(sub.id, sub);
        subscriptions.set(sub.id, bot.id);
        startTimers(st, rc);

        const receiver = receiverInfo(st);
        if (!receiver) throw '接收器没有建起来';
        return { subscription_id: sub.id, receiver };
    },

    /** 退订立刻生效，不排延迟：页面卸载时不该在浏览器里留定时器 */
    unsubscribe: (subscriptionId: string): Promise<void> => {
        const botId = subscriptions.get(subscriptionId);
        subscriptions.delete(subscriptionId);
        const st = botId ? states.get(botId) : undefined;
        const rc = st?.receiver;
        const sub = rc?.subs.get(subscriptionId);
        if (rc && sub) {
            if (sub.flushTimer !== null) clearTimeout(sub.flushTimer);
            rc.subs.delete(subscriptionId);
            if (rc.subs.size === 0) stopTimers(rc);
        }
        return Promise.resolve();
    },

    receivers: (): Promise<DebugReceiverInfo[]> => {
        const list: DebugReceiverInfo[] = [];
        for (const st of states.values()) {
            const info = receiverInfo(st);
            if (info) list.push(info);
        }
        return respond(list);
    },

    stopReceiver: (botId: string): Promise<void> => {
        const st = states.get(botId);
        if (st) shutdownReceiver(st, '已手动停止');
        return respond(undefined);
    },

    readEvents: (botId: string, sinceSeq: number, limit: number): Promise<DebugEvent[]> => {
        if (!findBot(botId)) return rejectAfterDelay(errorText({ kind: 'bot_not_found' }));
        const ring = states.get(botId)?.ring ?? [];
        return respond(clone(ring.filter((e) => e.seq > sinceSeq).slice(0, Math.max(0, limit))));
    },

    workspace: (): Promise<DebugWorkspace> => respond(clone(workspaceStore)),
    saveWorkspace: (workspace: DebugWorkspace): Promise<void> => {
        replaceWorkspace(clone(workspace));
        return respond(undefined);
    },

    collections: (): Promise<DebugCollections> => respond(clone(collectionsStore)),
    saveCollections: (collections: DebugCollections): Promise<void> => {
        replaceCollections(clone(collections));
        return respond(undefined);
    },
    exportCollections: (path: string): Promise<void> => {
        files.set(path, JSON.stringify(collectionsStore, null, 2));
        return respond(undefined);
    },
    /** 导入是并进现有收藏：id 撞了的改名，文件夹引用跟着改 */
    importCollections: (path: string): Promise<DebugCollections> => {
        const text = files.get(path);
        if (text === undefined) return rejectAfterDelay(`读不到文件：${path}`);
        let parsed: unknown;
        try {
            parsed = JSON.parse(text);
        } catch {
            return rejectAfterDelay('不是有效的收藏文件');
        }
        if (
            !isRecord(parsed) ||
            !Array.isArray(parsed.folders) ||
            !Array.isArray(parsed.requests)
        ) {
            return rejectAfterDelay('不是有效的收藏文件');
        }
        mergeCollections(parsed as unknown as DebugCollections);
        return respond(clone(collectionsStore));
    },

    history: (query: DebugHistoryQuery): Promise<DebugHistoryPage> => {
        const text = query.text?.trim().toLowerCase();
        const matched = historyStore.filter(
            (e) =>
                (!query.action || e.action === query.action) &&
                (!query.bot_id || e.bot_id === query.bot_id) &&
                (query.ok === null || query.ok === undefined || e.ok === query.ok) &&
                (!text ||
                    e.action.toLowerCase().includes(text) ||
                    e.bot_name.toLowerCase().includes(text) ||
                    JSON.stringify(e.params).toLowerCase().includes(text)),
        );
        const page = matched.slice(query.offset, query.offset + query.limit);
        return respond({
            entries: page.map((e) => ({
                id: e.id,
                at_ms: e.at_ms,
                bot_id: e.bot_id,
                bot_name: e.bot_name,
                backend: e.backend,
                channel: e.channel,
                origin: e.origin,
                action: e.action,
                ok: e.ok,
                retcode: e.retcode,
                elapsed_ms: e.elapsed_ms,
                error_kind: e.error ? e.error.kind : null,
            })),
            total: matched.length,
        });
    },
    historyEntry: (id: string): Promise<DebugHistoryEntry | null> => {
        const hit = historyStore.find((e) => e.id === id);
        return respond(hit ? clone(hit) : null);
    },
    clearHistory: clearHistoryStore,

    // 和后端一样「取走」语义：读过就清空，下一份损坏挪走再出现。预览里想看到横幅，
    // 控制台执行 `__ncdDebugStorageNotice()` 造一条
    storageNotices: takeStorageNotices,
};

/** 测试用：把所有状态、定时器、计数器恢复到刚加载的样子 */
export function resetOnebotDebugMock(): void {
    resetCallMock();
    resetEventMock();
    resetBotsMock();
    memberCache.clear();
    resetCollectionsMock();
}

// 浏览器预览里的压测口子：控制台执行 `__ncdDebugFlood(5000)`，往正在被看的 Bot 一口气灌 5000 条群消息。
// 只在没有 Tauri 的浏览器预览里挂（含 vite preview 出来的生产包，性能工具就是用它），真程序里不存在
if (typeof window !== 'undefined' && !isTauri) {
    (window as unknown as { __ncdDebugFlood?: typeof flood }).__ncdDebugFlood = flood;
    (window as unknown as { __ncdDebugStorageNotice?: () => void }).__ncdDebugStorageNotice =
        () => {
            pushStorageNotice({
                file: 'history.jsonl',
                moved_to: 'history.jsonl.broken-2026-09-30',
                reason: '第 3,812 行不是合法的 JSON',
            });
        };
}

// Bot 日志缓冲：按 botId 攒在模块级 store，日志页卸载不丢行。
// 旧实现把事件订阅挂在 useDomainEvents 里，随页面卸载断订，断订窗口里的
// bot_log_appended 永久丢；SL 的 daemon 增量对不上磁盘历史，丢了就整段「没日志」。
// 订阅在首个调用方挂时建立一次，之后永不卸载。
// bot 状态翻 running 算另起一轮（应用端实例有显式 reset 事件，这边没有，
// 拿状态翻转当等价物）：收到就清空，这一轮从开头看着收，开页不再拿盘上尾巴盖。

import { createStore } from '../utils/createStore';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import { botService } from '../../core/services/bot.service';
import {
    appendLine,
    buildHistoryEntries,
    normalizeChannel,
    snowlumaLineChannel,
    type LogChannel,
    type LogEntry,
} from '../../core/domain/events/log-buffer';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import type { BotActorState, DomainEvent } from '../../core/ipc/types';

interface State {
    byId: Record<string, LogEntry[]>;
}

const EMPTY: LogEntry[] = [];

const store = createStore<State>({ byId: {} });

let unsubDomain: (() => void) | null = null;

// botId → 后端类型。daemon 增量归属和 bot 通道里 [NapCat] 转发行的过滤都靠它分流。
const backendByBot = new Map<string, BackendType>();

// bot 最近一次 bot_state_changed 报的状态，用来识别「翻 running」。
// 桌面端打开前就在跑的 bot 我们没见过它的非 running 态，不会被误判成新轮次。
const lastStateByBot = new Map<string, BotActorState>();

// 这一轮是从开头看着收进来的：缓冲就是这一轮的全部，
// 开页不用再拿盘上的尾巴整块盖掉——盖了反而丢掉攒下的增量。
const watchedFromStart = new Set<string>();

function setLogs(botId: string, logs: LogEntry[]): void {
    store.setState({ byId: { ...store.getSnapshot().byId, [botId]: logs } });
}

function startNewRun(botId: string): void {
    watchedFromStart.add(botId);
    const { byId } = store.getSnapshot();
    if (byId[botId] === EMPTY) return;
    store.setState({ byId: { ...byId, [botId]: EMPTY } });
}

function appendBotLines(botId: string, line: string, channel: LogChannel): void {
    // 后端 EventBusSink 可能把同 channel 多行用 \n 合并成一条事件
    const rawLines = line.includes('\n') ? line.split('\n') : [line];
    const backend = backendByBot.get(botId) ?? null;
    const current = store.getSnapshot().byId[botId] ?? EMPTY;
    let next = current;
    for (const raw of rawLines) {
        if (backend === 'snowluma' && raw.includes('[NapCat]')) {
            continue;
        }
        next = appendLine(next, raw, channel);
    }
    if (next === current) return;
    setLogs(botId, next);
}

// daemon 日志不带 bot_id（旧实现只进当前打开的 SL bot 页），这里进所有
// 已识别为 snowluma 后端的 bot 缓冲：单个 bot 的日志页看到的与旧实现一致。
function appendDaemonLine(line: string): void {
    const channel = snowlumaLineChannel(line);
    for (const [botId, backend] of backendByBot) {
        if (backend !== 'snowluma') continue;
        const current = store.getSnapshot().byId[botId] ?? EMPTY;
        const next = appendLine(current, line, channel);
        if (next !== current) setLogs(botId, next);
    }
}

function onDomainEvent(event: DomainEvent): void {
    if (event.kind === 'bot_log_appended') {
        appendBotLines(event.bot_id, event.line, normalizeChannel(event.channel));
        return;
    }

    if (event.kind === 'snowluma_daemon_log') {
        appendDaemonLine(event.line);
        return;
    }

    if (event.kind === 'bot_state_changed') {
        const { bot_id: botId, state } = event.snapshot;
        const prev = lastStateByBot.get(botId);
        lastStateByBot.set(botId, state);
        if (state === 'running' && prev !== undefined && prev !== 'running') {
            startNewRun(botId);
        }
        return;
    }

    if (event.kind === 'bot_process_exited') {
        // 进程退了，这一轮结束：缓冲清空（沿用旧行为），下次开页从盘上重新补。
        // actor 随后还会自己报状态，但退出事件可能先到；先把上一态记成非 running，
        // 免得重启后翻 running 被当成「一直在跑」漏掉新轮次。
        watchedFromStart.delete(event.bot_id);
        lastStateByBot.set(event.bot_id, 'stopped');
        if ((store.getSnapshot().byId[event.bot_id] ?? EMPTY) === EMPTY) return;
        setLogs(event.bot_id, EMPTY);
    }
}

function ensureSubscribed(): void {
    if (unsubDomain) return;
    unsubDomain = subscribeDomainEvents(onDomainEvent);
}

const hydrating = new Set<string>();

export function hydrateBotLogs(botId: string): void {
    if (!botId) return;
    // 后端类型每次开页照旧拉一次（对齐旧 hook 的请求时机）：
    // SL 与否决定 daemon 增量归属与 bot 通道里 [NapCat] 转发行的过滤
    botService
        .getConfig(botId)
        .then((cfg) => {
            if (cfg) backendByBot.set(botId, cfg.bot.backend_type);
        })
        .catch(() => {});
    if (hydrating.has(botId) || watchedFromStart.has(botId)) return;
    hydrating.add(botId);
    void botService
        .tailLog(botId, 1000)
        .then((snapshot) => {
            // 拉的途中另起了一轮：这份是上一轮的
            if (watchedFromStart.has(botId)) return;
            const historical = buildHistoryEntries(snapshot.lines);
            if (historical.length === 0) {
                // 不盖空数组：远端 bot_*.log 滤完常为空，盖了就把攒下的增量清掉
                return;
            }
            setLogs(botId, historical);
        })
        .catch((err) => {
            // eslint-disable-next-line no-console
            console.warn('加载 Bot 历史日志失败:', err);
        })
        .finally(() => {
            hydrating.delete(botId);
        });
}

export function clearBotLogs(botId: string): void {
    const { byId } = store.getSnapshot();
    if (!byId[botId]?.length) return;
    store.setState({ byId: { ...byId, [botId]: EMPTY } });
}

function subscribe(listener: () => void): () => void {
    ensureSubscribed();
    return store.subscribe(listener);
}

export const botLogStore = {
    getSnapshot: store.getSnapshot,
    subscribe,
    _reset(): void {
        if (unsubDomain) {
            try {
                unsubDomain();
            } catch {
                /* noop */
            }
            unsubDomain = null;
        }
        hydrating.clear();
        backendByBot.clear();
        lastStateByBot.clear();
        watchedFromStart.clear();
        store._reset();
    },
};

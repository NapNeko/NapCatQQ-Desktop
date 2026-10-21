import type { BatchResultResponse, BotActorSnapshot, BotStatus, LogSnapshot } from '@desktop/core/ipc/types';
import type { BotConfig } from '@desktop/core/ipc/generated/domain/BotConfig';
import { createDefaultBotConfig } from '@desktop/core/domain/bot/config-defaults';
import { emitMockEvent } from './events.mock';

export const mockBots: BotStatus[] = [];
export const mockSnapshots: BotActorSnapshot[] = [];
const configs = new Map<string, BotConfig>();
const logs = new Map<string, string[]>();
const timers = new Map<string, Set<ReturnType<typeof setTimeout>>>();
const listeners = new Set<() => void>();
export const mockLogSnapshot: LogSnapshot = { lines: [], total_lines: 0 };

export function subscribeScenario(callback: () => void): () => void {
  listeners.add(callback);
  return () => { listeners.delete(callback); };
}

function notify() { for (const listener of listeners) listener(); }
const clone = <T,>(value: T): T => structuredClone(value);

export function demoReadConfig(botId: string): BotConfig | null {
  const config = configs.get(botId);
  return config ? clone(config) : null;
}

export function buildMockBotConfig(botId: string): BotConfig {
  return demoReadConfig(botId) ?? createDefaultBotConfig();
}

export function demoFlavors() {
  return Object.fromEntries([...configs].map(([id, config]) => [id, config.bot.backend_type]));
}

export function demoTailLog(botId: string, count = 1000): LogSnapshot {
  const lines = logs.get(botId) ?? [];
  return { lines: lines.slice(-count), total_lines: lines.length };
}

export function seedDemoBot(
  id = '10003',
  name = '我的第一个 Bot',
  state: BotActorSnapshot['state'] = 'stopped',
  lines?: string[],
): void {
  clearTimers(id);
  const config = createDefaultBotConfig();
  config.bot.QQID = Number(id);
  config.bot.name = name;
  configs.set(id, config);
  logs.set(id, lines?.slice() ?? ['[演示] 本机 NapCat 组件已就绪，此处仅模拟进程状态。']);
  const snapshot: BotActorSnapshot = {
    bot_id: id,
    state,
    revision: 1,
    token_generation: 1,
    pending_restart: false,
    last_transition: 'demo_seed',
  };
  const status: BotStatus = {
    bot_id: id,
    state,
    pid: state === 'running' ? 12048 : undefined,
    memory_rss_bytes: state === 'running' ? 120 * 1024 * 1024 : undefined,
    extra: {},
  };
  const snapshotIndex = mockSnapshots.findIndex(bot => bot.bot_id === id);
  const statusIndex = mockBots.findIndex(bot => bot.bot_id === id);
  if (snapshotIndex < 0) mockSnapshots.push(snapshot);
  else mockSnapshots[snapshotIndex] = snapshot;
  if (statusIndex < 0) mockBots.push(status);
  else mockBots[statusIndex] = status;
  notify();
}

export function mockRemoteNetwork() { return null; }

function clearTimers(botId: string) {
  for (const timer of timers.get(botId) ?? []) clearTimeout(timer);
  timers.delete(botId);
}

function later(botId: string, delay: number, action: () => void) {
  const pending = timers.get(botId) ?? new Set();
  timers.set(botId, pending);
  const timer = setTimeout(() => {
    pending.delete(timer);
    if (configs.has(botId)) action();
  }, delay);
  pending.add(timer);
}

function appendLog(botId: string, message: string) {
  const lines = logs.get(botId) ?? [];
  lines.push(message);
  logs.set(botId, lines);
  emitMockEvent({ kind: 'bot_log_appended', bot_id: botId, line: message, channel: 'stdout' });
}

function transition(snapshot: BotActorSnapshot, state: BotActorSnapshot['state'], reason: string) {
  snapshot.state = state;
  snapshot.revision += 1;
  snapshot.last_transition = reason;
  const status = mockBots.find(bot => bot.bot_id === snapshot.bot_id);
  if (status) {
    status.state = state;
    status.pid = state === 'running' ? 12048 : undefined;
    status.memory_rss_bytes = state === 'running' ? 120 * 1024 * 1024 : undefined;
  }
  emitMockEvent({ kind: 'bot_state_changed', snapshot: clone(snapshot), reason });
  notify();
}

export async function mockUpsertBotConfig(config: BotConfig): Promise<BotActorSnapshot> {
  const id = String(config.bot.QQID);
  configs.set(id, clone(config));
  let snapshot = mockSnapshots.find(bot => bot.bot_id === id);
  if (!snapshot) {
    snapshot = { bot_id: id, state: 'stopped', revision: 0, token_generation: 1, pending_restart: false };
    mockSnapshots.push(snapshot);
    mockBots.push({ bot_id: id, state: 'stopped', extra: {} });
    logs.set(id, ['[演示] 配置已保存在本页内存中。']);
  }
  transition(snapshot, snapshot.state, 'config_saved');
  return clone(snapshot);
}

export async function mockStartBot(botId: string): Promise<BotActorSnapshot> {
  const snapshot = mockSnapshots.find(bot => bot.bot_id === botId);
  if (!snapshot) throw new Error('请先保存 Bot 配置');
  if (snapshot.state === 'running' || snapshot.state === 'starting') return clone(snapshot);
  clearTimers(botId);
  logs.set(botId, []);
  transition(snapshot, 'starting', 'manual_start');
  appendLog(botId, '[演示] 正在模拟启动，不会运行 QQ 或协议端。');
  later(botId, 350, () => appendLog(botId, '[NapCat] 读取 Bot 配置完成'));
  later(botId, 850, () => appendLog(botId, '[NapCat] 本机 QQ 与 NapCat 组件已就绪'));
  later(botId, 1500, () => {
    transition(snapshot, 'running', 'startup_complete');
    appendLog(botId, '[NapCat] 协议端进程已启动');
  });
  later(botId, 2100, () => appendLog(botId, '[演示] 等待 QQ 登录。实际使用时请打开 WebUI，扫码并在手机确认。'));
  return clone(snapshot);
}

export async function mockStopBot(botId: string): Promise<BotActorSnapshot> {
  const snapshot = mockSnapshots.find(bot => bot.bot_id === botId);
  if (!snapshot) throw new Error('找不到这个 Bot');
  clearTimers(botId);
  transition(snapshot, 'stopping', 'manual_stop');
  appendLog(botId, '[演示] 正在停止协议端进程…');
  later(botId, 700, () => {
    transition(snapshot, 'stopped', 'shutdown_complete');
    appendLog(botId, '[演示] 进程已停止');
  });
  return clone(snapshot);
}

export async function mockDeleteBotConfig(botId: string): Promise<void> {
  clearTimers(botId);
  configs.delete(botId);
  logs.delete(botId);
  for (const list of [mockSnapshots, mockBots]) {
    const index = list.findIndex(bot => bot.bot_id === botId);
    if (index >= 0) list.splice(index, 1);
  }
  notify();
}

async function batch(ids: string[], action: (id: string) => Promise<unknown>): Promise<BatchResultResponse> {
  const result: BatchResultResponse = { succeeded: [], failed: [] };
  for (const id of ids) {
    try { await action(id); result.succeeded.push(id); }
    catch (error) { result.failed.push([id, String(error)]); }
  }
  return result;
}

export const mockBatchStart = (ids: string[]) => batch(ids, mockStartBot);
export const mockBatchStop = (ids: string[]) => batch(ids, mockStopBot);
export const mockBatchDelete = (ids: string[]) => batch(ids, mockDeleteBotConfig);

export function resetScenario() {
  for (const id of timers.keys()) clearTimers(id);
  configs.clear(); logs.clear();
  mockSnapshots.splice(0); mockBots.splice(0);
  notify();
}

// NapCatQQ Desktop P2 UI - IPC Events
import { listen } from '@tauri-apps/api/event';
import { isTauri } from './client';

export type EventCallback = (event: any) => void;
export type UnsubscribeFn = => void;

const activeMockCallbacks = new Set<EventCallback>;

// Helper to trigger custom mock events in standalone mode
export function emitMockEvent(event: any) {
 for (const cb of activeMockCallbacks) {
 try {
 cb(event);
 } catch (err) {
 console.error('Error invoking mock event callback:', err);
 }
 }
}

// Listen to all relevant Tauri events
export async function subscribeToEvents(callback: EventCallback): Promise<UnsubscribeFn> {
 if (isTauri) {
 const unlisteners: any[] = [];
 const eventNames = [
 'bot_state_changed',
 'bot_status_changed',
 'bot_log_appended',
 'bot_error',
 'task_progress',
 'napcat_webui_available',
 'bot_process_exited',
 'napcat_login_qrcode',
 'napcat_login_qrcode_removed',
 'napcat_login_online',
 'napcat_login_invalidated',
 // SnowLuma 系列
 'snowluma_daemon_state_changed',
 'snowluma_bot_injected',
 'snowluma_uin_detected',
 'snowluma_login_state_changed',
 'snowluma_pid_set_changed',
 'snowluma_daemon_log',
 ];

 for (const name of eventNames) {
 try {
 const unlisten = await listen<string>(name, (tauriEvent) => {
 try {
 // In Tauri v2, if Rust serialized with serde_json::to_string, payload is a JSON string
 const payloadStr = tauriEvent.payload;
 const parsed = typeof payloadStr === 'string' ? JSON.parse(payloadStr) : payloadStr;
 // 诊断日志：临时打印所有事件，确认链路是否通。稳定后可移除。
 // eslint-disable-next-line no-console
 console.log('[Tauri event]', name, parsed);
 callback(parsed);
 } catch (err) {
 console.error(`解析 Tauri 事件 ${name} payload 失败:`, err, tauriEvent);
 }
 });
 unlisteners.push(unlisten);
 } catch (err) {
 console.error(`监听 Tauri 事件 ${name} 失败:`, err);
 }
 }
 // eslint-disable-next-line no-console
 console.log('[Tauri events] subscribed to', eventNames.length, 'channels');

 return => {
 for (const unlisten of unlisteners) {
 unlisten;
 }
 };
 } else {
 activeMockCallbacks.add(callback);
 // Standalone Web View: generate elegant periodic mock events
 let isUnsubscribed = false;
 const intervalIds: any[] = [];

 // Emit initial event after 1 sec
 const t1 = setTimeout( => {
 if (isUnsubscribed) return;
 callback({
 kind: 'task_progress',
 task_id: 'boot-migration',
 progress: 100,
 message: '数据层 V2 -> V3 迁移检测完成。',
 });
 }, 1000);
 intervalIds.push(t1);

 // Emit log lines periodically
 const logInterval = setInterval( => {
 if (isUnsubscribed) return;
 const botIds = ['10001', '10002'];
 const randomBot = botIds[Math.floor(Math.random * botIds.length)];
 const logLines = [
 '[NapCat] [INFO] WebSocket service listening on port 3001',
 '[NapCat] [INFO] Connected to QQ server successfully',
 '[NapCat] [DEBUG] Syncing contacts... 45%',
 '[NapCat] [INFO] OneBot11 api call: get_login_info',
 '[NapCat] [WARN] Connection to gateway lost, retrying in 5s...',
 '[NapCat] [INFO] Reconnected to gateway',
 ];
 const line = logLines[Math.floor(Math.random * logLines.length)];
 callback({
 kind: 'bot_log_appended',
 bot_id: randomBot,
 line,
 channel: 'stdout',
 });
 }, 3500);
 intervalIds.push(logInterval);

 // Emit progress updates
 const progressInterval = setInterval( => {
 if (isUnsubscribed) return;
 const progress = Math.floor(Math.random * 100);
 callback({
 kind: 'task_progress',
 task_id: 'remote-tunnel',
 progress,
 message: `正在刷新远端隧道连接缓存 (${progress}%)`,
 });
 }, 8000);
 intervalIds.push(progressInterval);

 return => {
 isUnsubscribed = true;
 activeMockCallbacks.delete(callback);
 for (const id of intervalIds) {
 clearTimeout(id);
 clearInterval(id);
 }
 };
 }
}

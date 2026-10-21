import type { ChatArchive } from '@desktop/core/ipc/generated/chat/ChatArchive';
import type { ChatViewState } from '@desktop/core/ipc/generated/chat/ChatViewState';
import { BOTS } from '@desktop/core/ipc/mock/onebot-debug/bots';
import { defaultWorkspace, replaceWorkspace, replaceCollections } from '@desktop/core/ipc/mock/onebot-debug/collections';
import { resetOnebotDebugMock } from '@desktop/core/ipc/mock/onebot-debug.mock';
import { instances } from '@desktop/core/ipc/mock/app-framework/state';
import { mockAppConfigControls } from '@desktop/core/ipc/mock/app-config.mock';
import { debugWorkspaceStore } from '@desktop/hooks/debug/debugWorkspaceStore';
import { debugEventStore } from '@desktop/hooks/debug/debugEventStore';
import { noticeEventStore } from '@desktop/hooks/events/noticeEventStore';
import { appInstanceLogStore } from '@desktop/hooks/apps/appInstanceLogStore';
import { chatService } from '@desktop/core/services/chat.service';
import { chatArchiveService } from '@desktop/core/services/chat-archive.service';
import { chatDesktopService } from '@desktop/core/services/chat-desktop.service';
import { seedDemoBot } from './bot.mock';
import { patchDemoSettings } from './system.mock';

export const workspaceBotId = '10003';
export const workspaceBotName = '教程 Bot';
export const workspaceAppId = 'k1a2b3c4';
export const workspaceAppName = 'Karin · 本机';
const appSeed = structuredClone(instances.find(instance => instance.id === workspaceAppId)!);
const archives = new Map<string, ChatArchive>();
let chatGeneration = 0;
let messageId = 800000;
let view: ChatViewState = { v: 1, revision: 0, selectedBot: null, accounts: [] };
let installed = false;

export function installWorkspaceDemo() {
  if (installed) return;
  installed = true;
  chatArchiveService.load = async (botId, selfId) => {
    const saved = archives.get(JSON.stringify([botId, selfId]));
    return saved ? structuredClone(saved) : null;
  };
  chatArchiveService.save = async (botId, selfId, archive) => {
    archives.set(JSON.stringify([botId, selfId]), structuredClone(archive));
  };
  chatDesktopService.loadView = async () => structuredClone(view);
  chatDesktopService.saveView = async next => {
    if (next.revision === view.revision) view = structuredClone(next);
  };
  const status = chatDesktopService.status;
  chatDesktopService.status = async () => {
    const current = await status();
    return {
      ...current,
      accounts: current.accounts.map(account => ({
        ...account,
        preference: { ...account.preference, enabled: true, background: false, tray: false },
      })),
    };
  };
  const call = chatService.call;
  chatService.call = async (botId, action, params, requestId = crypto.randomUUID(), onProgress) => {
    if (!action.startsWith('send_')) return call(botId, action, params, requestId, onProgress);
    // The real composer settles its outgoing message from this in-memory receipt.
    return {
      request_id: requestId,
      result: {
        kind: 'ok',
        outcome: {
          ok: true, status: 'ok', retcode: 0, data: { message_id: ++messageId },
          message: '', wording: '教程模拟发送', raw: { status: 'ok', retcode: 0 },
          elapsed_ms: 12, channel: { kind: 'internal' }, size_bytes: 0, truncated: false,
        },
      },
    };
  };
  chatDesktopService.open = async () => { throw new Error('教程演示中请在当前窗口操作聊天。'); };
  chatDesktopService.close = async () => {};
}

export function resetWorkspaceDemo(lesson: 'debugging' | 'apps' | 'overview' | 'chat') {
  mockAppConfigControls.reset();
  resetOnebotDebugMock();
  debugEventStore._reset();
  debugWorkspaceStore._reset();
  noticeEventStore._reset();
  appInstanceLogStore._reset();
  const chatId = `docs-chat-${++chatGeneration}`;
  const id = lesson === 'chat' ? chatId : workspaceBotId;
  BOTS.splice(0, BOTS.length, {
    id, name: workspaceBotName, qq: 10003, backend: 'napcat', host: { kind: 'local' },
    running: true, online: true, tunnelPort: 0, httpRejectsToken: false,
    missingActions: new Set(), tokenHint: 'demo***',
  });
  const workspace = defaultWorkspace();
  workspace.layout.right_collapsed = true;
  workspace.layout.left_width = 230;
  replaceWorkspace(workspace);
  replaceCollections({ version: 1, folders: [], requests: [] });
  instances.splice(0, instances.length, { ...structuredClone(appSeed), link: undefined, auto_start: false });
  archives.clear();
  messageId = 800000;
  view = { v: 1, revision: chatGeneration, selectedBot: lesson === 'chat' ? chatId : null, accounts: [] };
  seedDemoBot(workspaceBotId, workspaceBotName, 'running', [
    '[演示] 本机 NapCat 已启动。',
    '[演示] QQ 登录完成，OneBot 内部通道已就绪。',
  ]);
  if (lesson === 'overview') {
    patchDemoSettings({ performanceMonitorEnabled: false });
    seedDemoBot('10004', '待检查的 Bot', 'crashed', [
      '[演示] 这是一次预设的异常退出场景。',
      '[演示] 请先核对最近日志，再决定是否重启。',
    ]);
  }
}

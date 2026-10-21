import type { ChatArchive } from '@desktop/core/ipc/generated/chat/ChatArchive';
import type { ChatViewState } from '@desktop/core/ipc/generated/chat/ChatViewState';
import { chatService } from '@desktop/core/services/chat.service';
import { chatArchiveService } from '@desktop/core/services/chat-archive.service';
import { chatDesktopService } from '@desktop/core/services/chat-desktop.service';

const archives = new Map<string, ChatArchive>();
let messageId = 800000;
let view: ChatViewState = { v: 1, revision: 0, selectedBot: null, accounts: [] };
let installed = false;

export function resetChatDemo(revision: number, selectedBot: string | null) {
  archives.clear();
  messageId = 800000;
  view = { v: 1, revision, selectedBot, accounts: [] };
}

export function installChatDemo() {
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

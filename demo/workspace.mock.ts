import { BOTS } from '@desktop/core/ipc/mock/onebot-debug/bots';
import { defaultWorkspace, replaceWorkspace, replaceCollections } from '@desktop/core/ipc/mock/onebot-debug/collections';
import { resetOnebotDebugMock } from '@desktop/core/ipc/mock/onebot-debug.mock';
import { instances } from '@desktop/core/ipc/mock/app-framework/state';
import { mockAppConfigControls } from '@desktop/core/ipc/mock/app-config.mock';
import { debugWorkspaceStore } from '@desktop/hooks/debug/debugWorkspaceStore';
import { debugEventStore } from '@desktop/hooks/debug/debugEventStore';
import { noticeEventStore } from '@desktop/hooks/events/noticeEventStore';
import { appInstanceLogStore } from '@desktop/hooks/apps/appInstanceLogStore';
import { seedDemoBot } from './bot.mock';
import { patchDemoSettings } from './system.mock';

export const workspaceBotId = '10003';
export const workspaceBotName = '教程 Bot';
export const workspaceAppId = 'k1a2b3c4';
export const workspaceAppName = 'Karin · 本机';
const appSeed = structuredClone(instances.find(instance => instance.id === workspaceAppId)!);
let chatGeneration = 0;
let selectedChatBot: string | null = null;
let chatDemo: typeof import('./chat.mock') | undefined;

export async function installWorkspaceDemo() {
  if (chatDemo) return;
  const loaded = await import('./chat.mock');
  if (chatDemo) return;
  loaded.installChatDemo();
  loaded.resetChatDemo(chatGeneration, selectedChatBot);
  chatDemo = loaded;
}

export function resetWorkspaceDemo(lesson: 'debugging' | 'apps' | 'overview' | 'chat' | 'terminal') {
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
  selectedChatBot = lesson === 'chat' ? chatId : null;
  chatDemo?.resetChatDemo(chatGeneration, selectedChatBot);
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

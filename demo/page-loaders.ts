export const pageLoaders = {
  bots: () => import('@desktop/modules/bot/BotPage.next').then(module => ({ default: module.BotPageNext })),
  components: () => import('@desktop/modules/components/ComponentsPage.next').then(module => ({ default: module.ComponentsPageNext })),
  settings: () => import('@desktop/modules/settings/SettingsPage.next').then(module => ({ default: module.SettingsPageNext })),
  overview: () => import('@desktop/modules/bootstrap/BootstrapPanel.next').then(module => ({ default: module.BootstrapPanelNext })),
  apps: () => import('@desktop/modules/apps/AppsPage.next').then(module => ({ default: module.AppsPageNext })),
  debug: () => import('@desktop/modules/debug/DebugConsolePage').then(module => ({ default: module.DebugConsolePage })),
  chat: async () => {
    const [page, mock] = await Promise.all([
      import('@desktop/modules/chat/ChatPage'),
      import('./workspace.mock'),
    ]);
    await mock.installWorkspaceDemo();
    return { default: page.ChatPage };
  },
  tasks: () => import('@desktop/modules/task-queue/TaskQueuePage.next').then(module => ({ default: module.TaskQueuePageNext })),
};

export const loadTerminalDock = () => import('@desktop/modules/terminal/TerminalDock').then(module => ({ default: module.TerminalDock }));

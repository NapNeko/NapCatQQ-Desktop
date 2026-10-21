import React, { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { useQueryClient } from '@tanstack/react-query';
import { AppProvidersNext } from '@desktop/app/AppProvidersNext';
import { BotPageNext } from '@desktop/modules/bot/BotPage.next';
import { Sidebar, type AppRoute } from '@desktop/shared/components/next/Sidebar';
import { TooltipProvider } from '@desktop/shared/ui/Tooltip';
import { InfoBarStack } from '@desktop/shared/ui/InfoBarStack';
import { GlobalTitleTooltip } from '@desktop/shared/ui/GlobalTitleTooltip';
import { useGlobalInfoBars } from '@desktop/hooks/ui/useGlobalInfoBars';
import { botLogStore } from '@desktop/hooks/bot/botLogStore';
import { globalInfoBarStore, pushInfoBar } from '@desktop/hooks/ui/globalInfoBarStore';
import { registerDesktopConsentHost } from '@desktop/hooks/desktop/desktopConsentHost';
import { featureTogglesStore, useFeatures } from '@desktop/hooks/preferences/featureTogglesStore';
import { preferencesStore } from '@desktop/core/domain/settings/preferencesStore';
import { MOTION_SPEED_DEFAULT } from '@desktop/core/design/motion';
import { botService } from '@desktop/core/services/bot.service';
import { componentService } from '@desktop/core/services/component.service';
import { useComponentActionEventBridge } from '@desktop/hooks/components/useComponentActionBridge';
import { useAppInstanceEventsBridge } from '@desktop/hooks/apps/useAppInstanceEventsBridge';
import { useDeploymentTaskBridge } from '@desktop/hooks/task-queue/useDeploymentTaskBridge';
import { useTaskQueue } from '@desktop/hooks/task-queue/useTaskQueue';
import { usePreferences } from '@desktop/hooks/preferences/usePreferences';
import { demoReadConfig, demoFlavors, demoTailLog, mockSnapshots, resetScenario } from './bot.mock';
import { installPlayer } from './player';
import { getLesson } from './lessons';
import { installSystemDemo, resetSystemDemo } from './system.mock';
import { installWorkspaceDemo } from './workspace.mock';
import './demo.css';

// Only this browser entry replaces services; the desktop source stays untouched.
Object.assign(botService, {
  listSnapshots: async () => structuredClone(mockSnapshots),
  getConfig: async (id: string) => demoReadConfig(id),
  listFlavors: async () => demoFlavors(),
  tailLog: async (id: string, lines?: number) => demoTailLog(id, lines),
});
const lesson = getLesson(new URLSearchParams(location.search).get('lesson'));
installSystemDemo();
installWorkspaceDemo();
const resolveOriginalReadiness = componentService.resolveRuntimeReadiness;
componentService.resolveRuntimeReadiness = async (componentId, hostId) => {
  const readiness = await resolveOriginalReadiness(componentId, hostId);
  if (lesson.route === 'bots' && hostId === 'local' && componentId === 'napcat') {
    for (const node of [readiness.root, ...readiness.plan.nodes]) {
      node.status = { state: 'satisfied', source: '教程演示环境' };
    }
  }
  return readiness;
};
registerDesktopConsentHost(async (action) => { await action?.(); return true; });
let parentTheme: 'light' | 'dark' = new URLSearchParams(location.search).get('theme') === 'dark' ? 'dark' : 'light';
function prepareLesson() {
  resetScenario();
  featureTogglesStore.apply({ napcat: true, snowluma: false, chat: true, apps: true, apiDebug: true, terminal: false, dockerPage: false, ncdWatch: false });
  preferencesStore.applySnapshot({
    theme: parentTheme, sidebarStyle: 'classic', showMascot: false,
    radiusStyle: 'standard', closeAction: 'close', motionSpeed: MOTION_SPEED_DEFAULT,
    motionEnabled: !matchMedia('(prefers-reduced-motion: reduce)').matches,
    motionLevel: 'standard',
  });
  resetSystemDemo();
  lesson.reset();
}
prepareLesson();

const taskHostLabels = { local: '本机' };
const ComponentsPage = lazy(() => import('@desktop/modules/components/ComponentsPage.next').then(module => ({ default: module.ComponentsPageNext })));
const SettingsPage = lazy(() => import('@desktop/modules/settings/SettingsPage.next').then(module => ({ default: module.SettingsPageNext })));
const OverviewPage = lazy(() => import('@desktop/modules/bootstrap/BootstrapPanel.next').then(module => ({ default: module.BootstrapPanelNext })));
const AppsPage = lazy(() => import('@desktop/modules/apps/AppsPage.next').then(module => ({ default: module.AppsPageNext })));
const DebugPage = lazy(() => import('@desktop/modules/debug/DebugConsolePage').then(module => ({ default: module.DebugConsolePage })));
const ChatPage = lazy(() => import('@desktop/modules/chat/ChatPage').then(module => ({ default: module.ChatPage })));
const TasksPage = lazy(() => import('@desktop/modules/task-queue/TaskQueuePage.next').then(module => ({ default: module.TaskQueuePageNext })));

function DemoShell() {
  const [collapsed, setCollapsed] = useState(true);
  const [pageKey, setPageKey] = useState(0);
  const [route, setRoute] = useState<AppRoute>(lesson.route);
  const queryClient = useQueryClient();
  const { bars, dismiss, remove } = useGlobalInfoBars();
  const { sidebarStyle } = usePreferences();
  const taskQueue = useTaskQueue({ hostLabels: taskHostLabels });
  const features = useFeatures();
  const hiddenRoutes = useMemo(() => {
    const hidden = new Set<AppRoute>(['docker', 'remote']);
    if (!features.apps) hidden.add('apps');
    if (!features.chat) hidden.add('chat');
    if (!features.apiDebug) hidden.add('debug');
    return hidden;
  }, [features.apps, features.chat, features.apiDebug]);
  useComponentActionEventBridge();
  useAppInstanceEventsBridge();
  useDeploymentTaskBridge();

  useEffect(() => installPlayer(lesson, {
    reset: () => {
      prepareLesson();
      botLogStore._reset();
      globalInfoBarStore.clear();
      queryClient.clear();
      setRoute(lesson.route);
      setCollapsed(true);
      setPageKey(key => key + 1);
    },
    theme: value => { parentTheme = value; preferencesStore.setTheme(value); },
  }), [queryClient]);

  function navigate(route: AppRoute) {
    if (!hiddenRoutes.has(route)) { setRoute(route); return; }
    pushInfoBar({
      tone: 'info', title: '当前演示没有打开这个入口',
      content: '可以重置演示，或在 Desktop 中操作。',
      key: 'demo-scope', autoDismissMs: 3500,
    });
  }

  return <TooltipProvider>
    <div className="demo-desktop flex h-screen w-screen overflow-hidden bg-canvas">
      <Sidebar active={route} onChange={navigate} collapsed={collapsed}
        sidebarStyle={sidebarStyle} onToggleCollapse={() => setCollapsed(value => !value)} hiddenRoutes={hiddenRoutes} taskQueueActiveCount={taskQueue.activeCount} />
      <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden px-5 pb-4 pt-4" data-demo-route={route}>
        <Suspense key={pageKey} fallback={<p className="p-6 text-sm text-text-secondary">正在打开页面…</p>}>
          {route === 'bots' && <BotPageNext onNavigate={navigate} />}
          {route === 'components' && <ComponentsPage />}
          {route === 'settings' && <SettingsPage />}
          {route === 'overview' && <OverviewPage onNavigate={navigate} />}
          {route === 'apps' && <AppsPage onNavigate={navigate} />}
          {route === 'debug' && <DebugPage onNavigate={navigate} />}
          {route === 'chat' && <ChatPage onNavigate={navigate} />}
          {route === 'tasks' && <TasksPage {...taskQueue} onNavigate={navigate} showDocker={false} />}
        </Suspense>
        <div id="ndf-float-layer" className="pointer-events-none absolute inset-0" />
      </main>
    </div>
    <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />
    <GlobalTitleTooltip />
    <div className="demo-cursor" aria-hidden="true" hidden>
      <svg width="27" height="32" viewBox="0 0 27 32" fill="none"><path d="M3 2v23l6-6 5 10 5-2-5-10h9L3 2Z" fill="#7656dc" stroke="white" strokeWidth="2.2" strokeLinejoin="round" /></svg>
      <span>演示中</span>
    </div>
  </TooltipProvider>;
}

createRoot(document.getElementById('root')!).render(<AppProvidersNext><DemoShell /></AppProvidersNext>);

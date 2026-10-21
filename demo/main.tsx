import React, { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { useQueryClient } from '@tanstack/react-query';
import { AppProvidersNext } from '@desktop/app/AppProvidersNext';
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
import { getLessonEntry } from './lessons';
import type { DemoLesson } from './lesson-types';
import { loadTerminalDock, pageLoaders } from './page-loaders';
import { installSystemDemo, resetSystemDemo } from './system.mock';
import './demo.css';

// Only this browser entry replaces services; the desktop source stays untouched.
Object.assign(botService, {
  listSnapshots: async () => structuredClone(mockSnapshots),
  getConfig: async (id: string) => demoReadConfig(id),
  listFlavors: async () => demoFlavors(),
  tailLog: async (id: string, lines?: number) => demoTailLog(id, lines),
});
const lessonEntry = getLessonEntry(new URLSearchParams(location.search).get('lesson'));
installSystemDemo();
const resolveOriginalReadiness = componentService.resolveRuntimeReadiness;
componentService.resolveRuntimeReadiness = async (componentId, hostId) => {
  const readiness = await resolveOriginalReadiness(componentId, hostId);
  if (lessonEntry.route === 'bots' && hostId === 'local' && componentId === 'napcat') {
    for (const node of [readiness.root, ...readiness.plan.nodes]) {
      node.status = { state: 'satisfied', source: '教程演示环境' };
    }
  }
  return readiness;
};
registerDesktopConsentHost(async (action) => { await action?.(); return true; });
let parentTheme: 'light' | 'dark' = new URLSearchParams(location.search).get('theme') === 'dark' ? 'dark' : 'light';
function prepareLesson(lesson: DemoLesson) {
  resetScenario();
  featureTogglesStore.apply({ napcat: true, snowluma: false, chat: true, apps: true, apiDebug: true, terminal: lesson.id === 'terminal', dockerPage: false, ncdWatch: false });
  preferencesStore.applySnapshot({
    theme: parentTheme, sidebarStyle: 'classic', showMascot: false,
    radiusStyle: 'standard', closeAction: 'close', motionSpeed: MOTION_SPEED_DEFAULT,
    motionEnabled: !matchMedia('(prefers-reduced-motion: reduce)').matches,
    motionLevel: 'standard',
  });
  resetSystemDemo();
  lesson.reset();
}

const taskHostLabels = { local: '本机' };
const BotPage = lazy(pageLoaders.bots);
const ComponentsPage = lazy(pageLoaders.components);
const SettingsPage = lazy(pageLoaders.settings);
const OverviewPage = lazy(pageLoaders.overview);
const AppsPage = lazy(pageLoaders.apps);
const DebugPage = lazy(pageLoaders.debug);
const ChatPage = lazy(pageLoaders.chat);
const TasksPage = lazy(pageLoaders.tasks);
const TerminalDock = lazy(loadTerminalDock);

function DemoShell({ lesson }: { lesson: DemoLesson }) {
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
      prepareLesson(lesson);
      botLogStore._reset();
      globalInfoBarStore.clear();
      queryClient.clear();
      setRoute(lesson.route);
      setCollapsed(true);
      setPageKey(key => key + 1);
    },
    theme: value => { parentTheme = value; preferencesStore.setTheme(value); },
  }), [lesson, queryClient]);

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
          {route === 'bots' && <BotPage onNavigate={navigate} />}
          {route === 'components' && <ComponentsPage />}
          {route === 'settings' && <SettingsPage />}
          {route === 'overview' && <OverviewPage onNavigate={navigate} />}
          {route === 'apps' && <AppsPage onNavigate={navigate} />}
          {route === 'debug' && <DebugPage onNavigate={navigate} />}
          {route === 'chat' && <ChatPage onNavigate={navigate} />}
          {route === 'tasks' && <TasksPage {...taskQueue} onNavigate={navigate} showDocker={false} />}
        </Suspense>
        <div id="ndf-float-layer" className="pointer-events-none absolute inset-0" />
        {lesson.id === 'terminal' && <Suspense fallback={null}><TerminalDock /></Suspense>}
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

async function start() {
  const pageLoader = pageLoaders[lessonEntry.route as keyof typeof pageLoaders];
  const [lesson] = await Promise.all([
    lessonEntry.load(), pageLoader(),
    lessonEntry.id === 'terminal' ? loadTerminalDock() : undefined,
  ]);
  prepareLesson(lesson);
  createRoot(document.getElementById('root')!).render(<AppProvidersNext><DemoShell lesson={lesson} /></AppProvidersNext>);
}

void start().catch(error => {
  console.error('教程演示加载失败', error);
  const message = '演示加载失败，请刷新后重试。';
  document.getElementById('root')!.textContent = message;
  window.parent.postMessage({
    channel: `ncd-docs-demo:${lessonEntry.id}`, type: 'state', mode: 'error',
    step: 0, title: '演示加载失败', detail: message, error: message,
  }, location.origin);
});

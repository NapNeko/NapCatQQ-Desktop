// 页面加载器由首点加载与侧栏预取共用；不持有界面状态。
import type { AppRoute } from '../../core/domain/ui/route';

// 与 lazy 共用同一 import 工厂，侧栏预取与首点加载同一 chunk。
export const loadBotPage = () =>
    import('../../modules/bot/BotPage.next').then((m) => ({ default: m.BotPageNext }));
export const loadAppsPage = () =>
    import('../../modules/apps/AppsPage.next').then((m) => ({ default: m.AppsPageNext }));
export const loadChatPage = () =>
    import('../../modules/chat/ChatPage').then((m) => ({ default: m.ChatPage }));
export const loadDebugPage = () =>
    import('../../modules/debug/DebugConsolePage').then((m) => ({ default: m.DebugConsolePage }));
export const loadComponentsPage = () =>
    import('../../modules/components/ComponentsPage.next').then((m) => ({
        default: m.ComponentsPageNext,
    }));
export const loadDockerPage = () =>
    import('../../modules/docker/DockerPage.next').then((m) => ({ default: m.DockerPageNext }));
export const loadRemotePage = () =>
    import('../../modules/remote/RemoteHostPanel.next').then((m) => ({
        default: m.RemoteHostPanelNext,
    }));
export const loadSettingsPage = () =>
    import('../../modules/settings/SettingsPage.next').then((m) => ({
        default: m.SettingsPageNext,
    }));
export const loadTaskQueuePage = () =>
    import('../../modules/task-queue/TaskQueuePage.next').then((m) => ({
        default: m.TaskQueuePageNext,
    }));

const ROUTE_PRELOAD: Partial<Record<AppRoute, () => Promise<unknown>>> = {
    bots: loadBotPage,
    apps: loadAppsPage,
    chat: loadChatPage,
    debug: loadDebugPage,
    components: loadComponentsPage,
    docker: loadDockerPage,
    remote: loadRemotePage,
    settings: loadSettingsPage,
    tasks: loadTaskQueuePage,
};

export function preloadRoute(route: AppRoute): void {
    const load = ROUTE_PRELOAD[route];
    if (load) void load();
}

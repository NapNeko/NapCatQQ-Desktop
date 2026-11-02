// 页面组合与任务页订阅；路由状态由 useAppNavigation 拥有。
import { Suspense, lazy, memo, useMemo, type ReactNode } from 'react';
import type { AppRoute } from '../../core/domain/ui/route';
import { BootstrapPanelNext } from '../../modules/bootstrap/BootstrapPanel.next';
import { useTaskQueue } from '../../hooks/task-queue/useTaskQueue';
import { useServers } from '../../hooks/remote/useServers';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import { Spinner } from '../../shared/ui/Spinner';
import {
    loadBotPage,
    loadAppsPage,
    loadChatPage,
    loadDebugPage,
    loadComponentsPage,
    loadDockerPage,
    loadRemotePage,
    loadSettingsPage,
    loadTaskQueuePage,
} from './pageLoaders';

const BotPageNext = lazy(loadBotPage);
const AppsPageNext = lazy(loadAppsPage);
const ChatPage = lazy(loadChatPage);
const DebugConsolePage = lazy(loadDebugPage);
const ComponentsPageNext = lazy(loadComponentsPage);
const DockerPageNext = lazy(loadDockerPage);
const RemoteHostPanelNext = lazy(loadRemotePage);
const SettingsPageNext = lazy(loadSettingsPage);
const TaskQueuePageNext = lazy(loadTaskQueuePage);

function RouteFallback() {
    return (
        <PagePlaceholder>
            <Spinner size="md" tone="brand" label="页面加载中" />
            <p className="text-[13px] text-text-secondary">正在加载页面…</p>
        </PagePlaceholder>
    );
}

/**
 * 页面本体。memo 住，props 只放稳定值：根组件因提示条、主机状态、引导等重渲时，
 * 当前页不跟着从头渲一遍。
 */
export const AppRoutes = memo(function AppRoutes({
    route,
    onNavigate,
    showDocker,
}: {
    route: AppRoute;
    onNavigate: (route: AppRoute) => void;
    showDocker: boolean;
}) {
    let body: ReactNode;
    switch (route) {
        case 'overview':
            body = <BootstrapPanelNext onNavigate={onNavigate} />;
            break;
        case 'bots':
            body = <BotPageNext onNavigate={onNavigate} />;
            break;
        case 'apps':
            body = <AppsPageNext onNavigate={onNavigate} />;
            break;
        case 'debug':
            body = <DebugConsolePage onNavigate={onNavigate} />;
            break;
        case 'chat':
            body = <ChatPage onNavigate={onNavigate} />;
            break;
        case 'components':
            body = <ComponentsPageNext />;
            break;
        case 'docker':
            body = <DockerPageNext />;
            break;
        case 'remote':
            body = <RemoteHostPanelNext />;
            break;
        case 'tasks':
            body = <TasksRoute onNavigate={onNavigate} showDocker={showDocker} />;
            break;
        case 'settings':
            body = <SettingsPageNext />;
            break;
        default: {
            const _exhaustive: never = route;
            void _exhaustive;
            body = null;
        }
    }

    // overview 同步；其余 lazy 包一层 Suspense，避免切页白屏。
    if (route === 'overview') {
        return body;
    }
    return <Suspense fallback={<RouteFallback />}>{body}</Suspense>;
});

/** 只有任务页要整份队列：订阅放在这里，进度事件只让任务页重渲。 */
function TasksRoute({
    onNavigate,
    showDocker,
}: {
    onNavigate: (route: AppRoute) => void;
    showDocker: boolean;
}) {
    // 导航常驻观察同一查询；进入任务页只读取它，不重复请求主机列表。
    const { data: servers } = useServers({ refetchOnMount: false });
    const hostLabels = useMemo(() => {
        const labels: Record<string, string> = { local: '本机' };
        for (const server of servers ?? [])
            labels[`remote:${server.id}`] = server.name?.trim() || server.host?.trim() || server.id;
        return labels;
    }, [servers]);
    const taskQueue = useTaskQueue({ hostLabels });
    return (
        <TaskQueuePageNext
            items={taskQueue.items}
            activeCount={taskQueue.activeCount}
            onNavigate={onNavigate}
            showDocker={showDocker}
        />
    );
}

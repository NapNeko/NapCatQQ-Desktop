// 主窗口导航拥有路由过渡和独立窗口让位；界面只消费状态和 navigate。
import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { AppRoute } from '../../core/domain/ui/route';
import { dockerProbeHosts } from '../../core/domain/docker/hosts';
import { localOs } from '../../core/domain/runtime/env';
import { featureTogglesStore, useFeatures } from '../../hooks/preferences/featureTogglesStore';
import { useDebugConsoleEnabled } from '../../hooks/debug/useDebugConsoleEnabled';
import { useServers } from '../../hooks/remote/useServers';
import { useAppWindowBridge } from '../../hooks/desktop/useAppWindowBridge';
import { registerDebugNavigator } from '../../hooks/debug/debugNav';
import { registerComponentsNavigator } from '../../hooks/components/componentsNavigation';
import { registerDockerNavigator } from '../../hooks/docker/dockerNavigation';
import { registerBotCreateNavigator } from '../../hooks/bot/botCreateNavigation';
import { preloadRoute } from './pageLoaders';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';

// 路由顺序,跟 Sidebar PRIMARY_NAV 对齐。PageTransition 用此判断切换方向。
const ROUTE_ORDER: ReadonlyArray<AppRoute> = [
    'overview',
    'bots',
    'chat',
    'apps',
    'debug',
    'components',
    'docker',
    'remote',
    'tasks',
    'settings',
];

export function useAppNavigation() {
    const navigationVersion = useRef(0);
    const pendingWindowRoute = useRef<AppRoute | null>(null);
    const [route, setRoute] = useState<AppRoute>('overview');
    const [displayedRoute, setDisplayedRoute] = useState<AppRoute>(route);
    const [pageVisible, setPageVisible] = useState<boolean>(true);
    // 独立窗口收回时同步切换目标与显示页，并作废旧导航请求。
    const showChatPage = useCallback(() => {
        if (!featureTogglesStore.getSnapshot().chat) return;
        navigationVersion.current += 1;
        setRoute('chat');
        setDisplayedRoute('chat');
        setPageVisible(true);
    }, []);
    const showChatPageQuiet = useCallback(() => {
        if (!featureTogglesStore.getSnapshot().chat) return;
        navigationVersion.current += 1;
        setRoute('chat');
        setDisplayedRoute('chat');
    }, []);
    const currentPage = useRef({ route, displayedRoute });
    currentPage.current = { route, displayedRoute };
    const releaseChatPage = useCallback(() => {
        const current = currentPage.current;
        if (current.route !== 'chat' && current.displayedRoute !== 'chat') return;
        const target = current.route === 'chat' ? 'overview' : current.route;
        const version = ++navigationVersion.current;
        pendingWindowRoute.current = null;
        setRoute(target);
        setDisplayedRoute(target);
        setPageVisible(true);
        // 用户已经在离开聊天时只完成原目标导航，不为旧页留下回滚。
        if (current.route !== 'chat') return;
        return () => {
            if (version === navigationVersion.current) showChatPage();
        };
    }, [showChatPage]);
    const { focusChatIfOpen, focusDebugIfOpen } = useAppWindowBridge({
        showChat: showChatPage,
        showChatQuiet: showChatPageQuiet,
        releaseChat: releaseChatPage,
    });
    const debugEnabled = useDebugConsoleEnabled();
    const { data: servers } = useServers();
    const dockerHostIds = useMemo(
        () =>
            dockerProbeHosts(
                localOs(),
                servers?.filter((p) => p.platform !== 'windows').map((p) => p.id) ?? [],
            ),
        [servers],
    );
    const features = useFeatures();
    // 切换引擎会清空状态缓存，入口不能因探测中的空值或失败而消失。
    const showDocker = features.dockerPage && dockerHostIds.length > 0;
    // 侧栏不显示、也不许切过去的页
    const hiddenRoutes = useMemo(() => {
        const hidden = new Set<AppRoute>();
        if (!showDocker) hidden.add('docker');
        if (!features.apps) hidden.add('apps');
        if (!features.chat) hidden.add('chat');
        if (!debugEnabled) hidden.add('debug');
        return hidden;
    }, [showDocker, features.apps, features.chat, debugEnabled]);
    // 只取消目标已被禁用的请求，其他功能变化不应吞掉仍有效的导航。
    useEffect(() => {
        if (pendingWindowRoute.current && hiddenRoutes.has(pendingWindowRoute.current)) {
            navigationVersion.current += 1;
            pendingWindowRoute.current = null;
        }
    }, [hiddenRoutes]);
    // 卸载后，旧窗口探测不能再提交导航。
    useEffect(
        () => () => {
            navigationVersion.current += 1;
        },
        [],
    );
    useEffect(() => {
        if (hiddenRoutes.has(route)) {
            startTransition(() => setRoute('overview'));
        }
    }, [hiddenRoutes, route]);

    const navigate = useCallback(
        (nextRoute: AppRoute) => {
            const version = ++navigationVersion.current;
            const target = hiddenRoutes.has(nextRoute) ? 'overview' : nextRoute;
            pendingWindowRoute.current = null;
            // 调试台弹出窗开着时主窗不进调试页（工作区 / 收藏落盘 JSON 是两窗同一份文件，
            // 两边同时写会互相盖），入口一律把弹出窗叫到前面；没开着才正常切路由
            if (target === 'chat' || target === 'debug') {
                pendingWindowRoute.current = target;
                const focus = target === 'chat' ? focusChatIfOpen : focusDebugIfOpen;
                void focus()
                    .then((focused) => {
                        if (version !== navigationVersion.current) return;
                        pendingWindowRoute.current = null;
                        if (
                            !focused &&
                            (target !== 'chat' || featureTogglesStore.getSnapshot().chat)
                        )
                            setRoute(target);
                    })
                    .catch((error) => {
                        if (version !== navigationVersion.current) return;
                        pendingWindowRoute.current = null;
                        pushErrorBar({
                            title: target === 'chat' ? '无法打开聊天' : '无法打开调试台',
                            raw: errorText(error),
                        });
                    });
                return;
            }
            // 侧栏点击必须是紧急更新：详情页一旦有持续 setState，startTransition 会一直交不出去。
            setRoute(target);
        },
        [hiddenRoutes, focusChatIfOpen, focusDebugIfOpen],
    );

    const prefetchRoute = useCallback(
        (nextRoute: AppRoute) => {
            if (hiddenRoutes.has(nextRoute)) return;
            preloadRoute(nextRoute);
        },
        [hiddenRoutes],
    );

    // Bot 卡片「调试」按钮 / 右键「在调试台打开」经 debugNav 跳过来
    useEffect(() => registerDebugNavigator(() => navigate('debug')), [navigate]);
    useEffect(() => registerComponentsNavigator(() => navigate('components')), [navigate]);
    useEffect(() => registerDockerNavigator(() => navigate('docker')), [navigate]);
    useEffect(() => registerBotCreateNavigator(() => navigate('bots')), [navigate]);

    const [direction, setDirection] = useState<-1 | 0 | 1>(0);

    useEffect(() => {
        if (route === displayedRoute) {
            if (!pageVisible) setPageVisible(true);
            return;
        }
        const oldIdx = ROUTE_ORDER.indexOf(displayedRoute);
        const newIdx = ROUTE_ORDER.indexOf(route);
        const dir: -1 | 0 | 1 =
            oldIdx < 0 || newIdx < 0 ? 0 : newIdx > oldIdx ? 1 : newIdx < oldIdx ? -1 : 0;
        setDirection(dir);
        setPageVisible(false);
    }, [route, displayedRoute, pageVisible]);

    const handlePageExited = () => {
        setDisplayedRoute(route);
        setPageVisible(true);
    };

    return {
        route,
        displayedRoute,
        pageVisible,
        direction,
        navigate,
        prefetchRoute,
        hiddenRoutes,
        showDocker,
        onPageExited: handlePageExited,
    };
}

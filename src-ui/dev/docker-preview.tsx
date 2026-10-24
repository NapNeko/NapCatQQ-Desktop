// 浏览器专用冒烟入口，不加入生产构建，也不保存 SSH 档案。
import { useEffect, useState, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DockerPageNext } from '../modules/docker/DockerPage.next';
import { ComponentsPageNext } from '../modules/components/ComponentsPage.next';
import { BotPageNext } from '../modules/bot/BotPage.next';
import { TooltipProvider, InfoBarStack } from '../shared/ui';
import { globalInfoBarStore } from '../hooks/ui/globalInfoBarStore';
import { registerComponentsNavigator } from '../hooks/components/componentsNavigation';
import { registerDockerNavigator } from '../hooks/docker/dockerNavigation';
import { registerBotCreateNavigator } from '../hooks/bot/botCreateNavigation';
import { registerDesktopConsentHost } from '../hooks/desktop/desktopConsentHost';
import { mockDockerStatus } from '../core/ipc/mock/docker.mock';
import type { ServerProfile } from '../core/ipc/generated/domain/ServerProfile';
import '../app/index.css';

const servers: ServerProfile[] = ['production', 'dev'].map((id) => ({
    id,
    name: `预览主机 ${id.toUpperCase()}`,
    host: `${id}.example.test`,
    port: 22,
    username: 'preview',
    authMethod: 'key',
    rememberCredential: false,
    state: 'connected',
}));
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
client.setQueryDefaults(['servers'], { staleTime: Infinity });
client.setQueryData(['servers'], servers);

const scenario = new URLSearchParams(location.search).get('status');
if (scenario === 'missing') {
    Object.assign(mockDockerStatus, {
        installed: false,
        version: '',
        daemonRunning: false,
        composeAvailable: false,
    });
} else if (scenario === 'stopped') {
    mockDockerStatus.daemonRunning = false;
} else if (scenario === 'no-compose') {
    mockDockerStatus.composeAvailable = false;
}

export function Preview() {
    const [route, setRoute] = useState<'docker' | 'components' | 'bots'>('docker');
    useEffect(() => registerComponentsNavigator(() => setRoute('components')), []);
    useEffect(() => registerDockerNavigator(() => setRoute('docker')), []);
    useEffect(() => registerBotCreateNavigator(() => setRoute('bots')), []);
    useEffect(() => {
        registerDesktopConsentHost(async (action) => {
            await action?.();
            return true;
        });
        return () => registerDesktopConsentHost(null);
    }, []);
    const { bars } = useSyncExternalStore(
        globalInfoBarStore.subscribe,
        globalInfoBarStore.getSnapshot,
    );
    return (
        <QueryClientProvider client={client}>
            <TooltipProvider>
                <main className="flex h-screen flex-col bg-canvas px-6 py-4">
                    <p className="mb-3 shrink-0 text-xs text-text-tertiary">
                        浏览器模拟数据 · 不连接实际 Docker
                    </p>
                    {route === 'docker' ? (
                        <DockerPageNext />
                    ) : route === 'components' ? (
                        <ComponentsPageNext />
                    ) : (
                        <BotPageNext />
                    )}
                </main>
                <InfoBarStack items={bars} onDismiss={globalInfoBarStore.dismiss} />
            </TooltipProvider>
        </QueryClientProvider>
    );
}

if (import.meta.env.DEV) createRoot(document.getElementById('root')!).render(<Preview />);

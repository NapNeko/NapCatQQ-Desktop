// 浏览器专用冒烟入口，不加入生产构建，也不保存 SSH 档案。
import { useState, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DockerHostPage } from '../modules/docker/DockerPage.next';
import { TooltipProvider, InfoBarStack } from '../shared/ui';
import { globalInfoBarStore } from '../hooks/ui/globalInfoBarStore';
import type { ServerProfile } from '../core/ipc/generated/domain/ServerProfile';
import '../app/index.css';

const servers: ServerProfile[] = ['a', 'b'].map((id) => ({
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

export function Preview() {
    const [hostId, setHostId] = useState('remote:a');
    const { bars } = useSyncExternalStore(
        globalInfoBarStore.subscribe,
        globalInfoBarStore.getSnapshot,
    );
    return (
        <QueryClientProvider client={client}>
            <TooltipProvider>
                <main className="flex h-screen flex-col bg-canvas px-6 py-4">
                    <p className="mb-2 text-xs text-text-tertiary">
                        浏览器模拟数据 · 不连接实际 Docker
                    </p>
                    <DockerHostPage
                        key={hostId}
                        hostId={hostId}
                        servers={servers}
                        onChangeHost={setHostId}
                    />
                </main>
                <InfoBarStack items={bars} onDismiss={globalInfoBarStore.dismiss} />
            </TooltipProvider>
        </QueryClientProvider>
    );
}

if (import.meta.env.DEV) createRoot(document.getElementById('root')!).render(<Preview />);

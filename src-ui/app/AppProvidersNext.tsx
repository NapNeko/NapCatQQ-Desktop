// 新 UI 树根的 provider 容器。
// react-query 在 dev HMR 时复用同一 client，避免每次全页刷新后 IPC 从零拉一遍。

import React from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

function createQueryClient() {
    return new QueryClient({
        defaultOptions: {
            queries: {
                refetchOnWindowFocus: false,
                retry: false,
                // dev 下缓存稍长，HMR 全刷后若 client 被复用可秒出旧数据
                staleTime: import.meta.env.DEV ? 30_000 : 0,
            },
        },
    });
}

type HotData = { queryClient?: QueryClient };

function getQueryClient(): QueryClient {
    const hot = import.meta.hot;
    if (hot) {
        const data = hot.data as HotData;
        if (!data.queryClient) {
            data.queryClient = createQueryClient();
        }
        return data.queryClient;
    }
    return createQueryClient();
}

const queryClient = getQueryClient();

export const AppProvidersNext: React.FC<{ children: React.ReactNode }> = ({ children }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
);

export default AppProvidersNext;
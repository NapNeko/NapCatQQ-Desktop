// 新 UI 树根的 provider 容器。
// 蓝绿模式下不复用 `app/providers/AppProviders.tsx`（那个绑了 FluentProvider，
// 新树不需要 Fluent）。
//
// 只挂 react-query：useBootstrap / useReleases 等 hook 都基于 useQuery。

import React from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            refetchOnWindowFocus: false,
            retry: false,
        },
    },
});

export const AppProvidersNext: React.FC<{ children: React.ReactNode }> = ({ children }) => (
    <QueryClientProvider client={queryClient}>
        {children}
    </QueryClientProvider>
);

export default AppProvidersNext;

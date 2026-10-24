// 组件操作完成后丢弃旧探测请求，再刷新共享查询。
import type { QueryClient } from '@tanstack/react-query';
import { LOCAL_QQ_SOURCE_QUERY_KEY } from './useLocalQqSource';
import { RUNTIME_READINESS_QUERY_KEY } from './useRuntimeReadiness';

const componentStateKeys = [
    [RUNTIME_READINESS_QUERY_KEY],
    ['componentDetect'],
    LOCAL_QQ_SOURCE_QUERY_KEY,
] as const;

export async function refreshComponentQueries(queryClient: QueryClient): Promise<void> {
    // 首次探测还没有 data 时，单独 invalidate 会复用安装前尚未返回的 Promise。
    await Promise.all(
        componentStateKeys.map((queryKey) => queryClient.cancelQueries({ queryKey })),
    );
    await Promise.all(
        [...componentStateKeys, ['servers'], ['docker'], ['componentCatalog']].map((queryKey) =>
            queryClient.invalidateQueries({ queryKey }),
        ),
    );
}

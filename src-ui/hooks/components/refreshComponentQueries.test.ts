import { QueryClient, QueryObserver } from '@tanstack/react-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { refreshComponentQueries } from './refreshComponentQueries';

const MISSING = { ready: false };
const READY = { ready: true };
const readinessKey = ['runtimeReadiness', 'remote:one', 'napcat'] as const;
const detectKey = ['componentDetect', 'qq', 'remote:one'] as const;
const clients: QueryClient[] = [];
const subscriptions: Array<() => void> = [];

function createClient() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: Infinity, gcTime: Infinity } },
    });
    clients.push(client);
    return client;
}

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((done) => {
        resolve = done;
    });
    return { promise, resolve };
}

function observe(
    client: QueryClient,
    queryKey: readonly unknown[],
    queryFn: () => Promise<typeof READY>,
) {
    const observer = new QueryObserver(client, { queryKey, queryFn });
    subscriptions.push(observer.subscribe(() => {}));
    return observer;
}

afterEach(() => {
    subscriptions.splice(0).forEach((unsubscribe) => unsubscribe());
    clients.splice(0).forEach((client) => client.clear());
});

describe('refreshComponentQueries', () => {
    it.each([
        { name: '运行预检', queryKey: readinessKey },
        { name: '组件探测', queryKey: detectKey },
        { name: '本机 QQ 来源', queryKey: ['localQqSource'] },
    ])('取消尚无数据的旧$name请求，旧结果晚回不会覆盖安装后结果', async ({ queryKey }) => {
        const client = createClient();
        const oldRequest = deferred<typeof READY>();
        const queryFn = vi
            .fn<() => Promise<typeof READY>>()
            .mockImplementationOnce(() => oldRequest.promise)
            .mockResolvedValue(READY);
        observe(client, queryKey, queryFn);
        expect(queryFn).toHaveBeenCalledTimes(1);
        expect(client.getQueryData(queryKey)).toBeUndefined();
        expect(client.getQueryState(queryKey)?.fetchStatus).toBe('fetching');

        await refreshComponentQueries(client);

        expect(queryFn).toHaveBeenCalledTimes(2);
        expect(client.getQueryData(queryKey)).toEqual(READY);
        oldRequest.resolve(MISSING);
        await oldRequest.promise;
        await Promise.resolve();
        expect(client.getQueryData(queryKey)).toEqual(READY);
    });

    it('刷新所有主机已有的 missing 缓存，同时保留无关查询', async () => {
        const client = createClient();
        const keys = [readinessKey, ['runtimeReadiness', 'remote:two', 'snowluma'], detectKey];
        const requests = keys.map((queryKey) => {
            client.setQueryData(queryKey, MISSING);
            const queryFn = vi.fn().mockResolvedValue(READY);
            observe(client, queryKey, queryFn);
            return queryFn;
        });
        const unrelatedKey = ['botConfig', 'one'];
        client.setQueryData(unrelatedKey, MISSING);
        const unrelatedRequest = vi.fn().mockResolvedValue(READY);
        observe(client, unrelatedKey, unrelatedRequest);

        await refreshComponentQueries(client);

        keys.forEach((queryKey, index) => {
            expect(requests[index]).toHaveBeenCalledOnce();
            expect(client.getQueryData(queryKey)).toEqual(READY);
        });
        expect(unrelatedRequest).not.toHaveBeenCalled();
        expect(client.getQueryData(unrelatedKey)).toEqual(MISSING);
    });

    it('未挂载的预检保留缓存但标记陈旧，重挂载后重新探测', async () => {
        const client = createClient();
        const queryFn = vi.fn().mockResolvedValue(READY);
        client.setQueryData(readinessKey, MISSING);

        await refreshComponentQueries(client);

        expect(client.getQueryData(readinessKey)).toEqual(MISSING);
        expect(client.getQueryState(readinessKey)?.isInvalidated).toBe(true);
        const observer = new QueryObserver(client, { queryKey: readinessKey, queryFn });
        await new Promise<void>((resolve) => {
            subscriptions.push(
                observer.subscribe((result) => {
                    if (result.data?.ready === true) resolve();
                }),
            );
        });
        expect(queryFn).toHaveBeenCalledOnce();
        expect(client.getQueryData(readinessKey)).toEqual(READY);
    });

    it('继续刷新服务器、Docker 容器、组件目录和本机 QQ 来源', async () => {
        const client = createClient();
        const keys = [
            ['servers'],
            ['docker', 'probe', 'remote:one'],
            ['docker', 'containers', 'remote:one'],
            ['componentCatalog'],
            ['localQqSource'],
        ];
        const requests = keys.map((queryKey) => {
            client.setQueryData(queryKey, MISSING);
            const queryFn = vi.fn().mockResolvedValue(READY);
            observe(client, queryKey, queryFn);
            return queryFn;
        });

        await refreshComponentQueries(client);

        keys.forEach((queryKey, index) => {
            expect(requests[index]).toHaveBeenCalledOnce();
            expect(client.getQueryData(queryKey)).toEqual(READY);
        });
    });
});

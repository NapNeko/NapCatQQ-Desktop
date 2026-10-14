import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotRuntimeMetrics } from '../../core/ipc/generated/domain/BotRuntimeMetrics';

const mocked = vi.hoisted(() => ({ list: vi.fn(), settings: vi.fn() }));
vi.mock('../../core/services/bot.service', () => ({
    botService: { listRuntimeMetrics: mocked.list },
}));
vi.mock('../../core/services/settings.service', () => ({
    settingsService: { get: mocked.settings },
}));

function metrics(collectedAt = 1000): BotRuntimeMetrics {
    return {
        v: 1,
        bot_id: '1',
        collected_at_ms: collectedAt,
        source: 'probe',
        probe: 'active',
        nodes: [],
    };
}

const settings = (enabled = true) => ({
    botRuntimeMetricsEnabled: enabled,
    botRuntimeMetricsIntervalMs: 1000,
    botRuntimeMetricsRetentionDays: 7,
});

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((ok) => {
        resolve = ok;
    });
    return { promise, resolve };
}

async function settle() {
    await vi.advanceTimersByTimeAsync(0);
}

let catalog: typeof import('./botRuntimeMetricsCatalog');
let unsubscribe: (() => void) | undefined;

beforeEach(async () => {
    vi.useFakeTimers();
    vi.resetModules();
    mocked.list.mockReset().mockResolvedValue([metrics()]);
    mocked.settings.mockReset().mockResolvedValue(settings());
    catalog = await import('./botRuntimeMetricsCatalog');
});

afterEach(() => {
    unsubscribe?.();
    unsubscribe = undefined;
    vi.useRealTimers();
});

describe('botRuntimeMetricsCatalog', () => {
    it('首拉、窗口可见性变化与手动刷新共用在途列表请求', async () => {
        const first = deferred<BotRuntimeMetrics[]>();
        mocked.list.mockReturnValueOnce(first.promise);
        unsubscribe = catalog.subscribeBotRuntimeMetricsCatalog(vi.fn());
        await settle();
        document.dispatchEvent(new Event('visibilitychange'));
        const refresh = catalog.refreshBotRuntimeMetricsCatalog();
        await settle();
        expect(mocked.list).toHaveBeenCalledTimes(1);
        first.resolve([metrics()]);
        await refresh;
        await settle();
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().fetching).toBe(false);
        await vi.advanceTimersByTimeAsync(1000);
        expect(mocked.list).toHaveBeenCalledTimes(2);
    });

    it('列表请求失败会释放在途标记，下一轮恢复采集', async () => {
        mocked.list.mockRejectedValueOnce(new Error('SSH 暂时不可用'));
        unsubscribe = catalog.subscribeBotRuntimeMetricsCatalog(vi.fn());
        await settle();
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().fetching).toBe(false);
        await vi.advanceTimersByTimeAsync(1000);
        expect(mocked.list).toHaveBeenCalledTimes(2);
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().byId['1']).toEqual(metrics());
    });

    it('首次读取设置失败后会重试，不能永久停留在未启用', async () => {
        mocked.settings.mockRejectedValueOnce(new Error('IPC 暂时不可用'));
        unsubscribe = catalog.subscribeBotRuntimeMetricsCatalog(vi.fn());
        await settle();
        expect(mocked.list).not.toHaveBeenCalled();
        // 首次设置读取失败时仍使用默认的 3 秒间隔。
        await vi.advanceTimersByTimeAsync(3000);
        expect(mocked.settings).toHaveBeenCalledTimes(2);
        expect(mocked.list).toHaveBeenCalledTimes(1);
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().enabled).toBe(true);
    });

    it('重新读取设置后，旧列表的迟到回包不能覆盖新列表或清掉新请求状态', async () => {
        const oldList = deferred<BotRuntimeMetrics[]>();
        const newList = deferred<BotRuntimeMetrics[]>();
        mocked.list.mockReturnValueOnce(oldList.promise).mockReturnValueOnce(newList.promise);
        unsubscribe = catalog.subscribeBotRuntimeMetricsCatalog(vi.fn());
        await settle();
        const reload = catalog.reloadBotRuntimeMetricsCatalogSettings();
        await settle();
        expect(mocked.list).toHaveBeenCalledTimes(2);
        oldList.resolve([metrics(1000)]);
        await settle();
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().byId).toEqual({});
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().fetching).toBe(true);
        newList.resolve([metrics(2000)]);
        await reload;
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().byId['1']).toEqual(metrics(2000));
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().fetching).toBe(false);
    });

    it('关闭监控后，之前发出的列表请求不能重新填回数据', async () => {
        const oldList = deferred<BotRuntimeMetrics[]>();
        mocked.list.mockReturnValueOnce(oldList.promise);
        unsubscribe = catalog.subscribeBotRuntimeMetricsCatalog(vi.fn());
        await settle();
        mocked.settings.mockResolvedValue(settings(false));
        await catalog.reloadBotRuntimeMetricsCatalogSettings();
        oldList.resolve([metrics()]);
        await settle();
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot()).toMatchObject({
            enabled: false,
            byId: {},
            fetching: false,
        });
    });

    it('较早的设置请求不能把较新的关闭设置改回开启', async () => {
        const oldSettings = deferred<ReturnType<typeof settings>>();
        mocked.settings.mockReturnValueOnce(oldSettings.promise).mockResolvedValue(settings(false));
        unsubscribe = catalog.subscribeBotRuntimeMetricsCatalog(vi.fn());
        await settle();
        await catalog.reloadBotRuntimeMetricsCatalogSettings();
        oldSettings.resolve(settings(true));
        await settle();
        expect(catalog.getBotRuntimeMetricsCatalogSnapshot().enabled).toBe(false);
        expect(mocked.list).not.toHaveBeenCalled();
    });
});

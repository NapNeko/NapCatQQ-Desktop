// Tabs 卸掉商店页；市场与已装拆开缓存，切走再进不重拉。

import type { AppStoreResource } from '../../core/ipc/types';

export const APP_STORE_STALE_MS = Number.POSITIVE_INFINITY;
export const APP_STORE_GC_MS = Number.POSITIVE_INFINITY;

export const appStoreMarketKey = (frameworkId: string, resource: AppStoreResource) =>
    ['appStoreMarket', frameworkId, resource] as const;

export const appStoreInstalledKey = (instanceId: string, resource: AppStoreResource) =>
    ['appStoreInstalled', instanceId, resource] as const;

export const karinPluginMarketKey = ['karinPluginMarket'] as const;

export const karinPluginsInstalledKey = (instanceId: string) =>
    ['karinPluginsInstalled', instanceId] as const;

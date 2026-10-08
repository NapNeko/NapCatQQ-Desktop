// SnowLuma 全局 WebUI 配置的加载 / 提交，从配置页壳里挪出来。
// services 直连仍留在白名单主文件：load / persist 由调用方注入，
// 这里不新增 core/services import。

import { useCallback, useEffect, useState } from 'react';
import type { SnowLumaAppConfig } from '../../../../core/ipc/generated/domain/SnowLumaAppConfig';
import { pushErrorBar } from '../../../../hooks/ui/pushErrorBar';

const defaultSnowlumaAppConfig = (): SnowLumaAppConfig => ({
    snowlumaWebuiPasswordOverride: '',
    snowlumaWebuiPort: 5099,
});

export interface BotConfigSnowlumaAppDeps {
    load: () => Promise<SnowLumaAppConfig>;
    persist: (config: SnowLumaAppConfig) => Promise<void>;
}

export interface BotConfigSnowlumaApp {
    snowlumaApp: SnowLumaAppConfig;
    setSnowlumaApp: (next: SnowLumaAppConfig) => void;
    snowlumaAppPristine: SnowLumaAppConfig;
    snowlumaAppLoading: boolean;
    snowlumaAppLoadError: string | null;
    commitIfDirty: () => Promise<void>;
}

export function useBotConfigSnowlumaApp(deps: BotConfigSnowlumaAppDeps): BotConfigSnowlumaApp {
    const { load, persist } = deps;
    const [snowlumaApp, setSnowlumaApp] = useState<SnowLumaAppConfig>(defaultSnowlumaAppConfig);
    const [snowlumaAppPristine, setSnowlumaAppPristine] =
        useState<SnowLumaAppConfig>(defaultSnowlumaAppConfig);
    const [snowlumaAppLoading, setSnowlumaAppLoading] = useState(true);
    const [snowlumaAppLoadError, setSnowlumaAppLoadError] = useState<string | null>(null);

    // service 方法是模块级稳定引用，加载只在挂载时发生一次，与原页面内联行为一致。
    useEffect(() => {
        let cancelled = false;
        (async () => {
            setSnowlumaAppLoading(true);
            setSnowlumaAppLoadError(null);
            try {
                const loaded = await load();
                if (cancelled) return;
                setSnowlumaApp(loaded);
                setSnowlumaAppPristine(loaded);
            } catch (e) {
                if (!cancelled) {
                    const raw = String(e);
                    setSnowlumaAppLoadError(raw);
                    pushErrorBar({
                        key: 'snowluma-app-load',
                        title: '无法加载全局 WebUI 配置',
                        raw,
                    });
                }
            } finally {
                if (!cancelled) setSnowlumaAppLoading(false);
            }
        })();
        return () => {
            cancelled = true;
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const commitIfDirty = useCallback(async (): Promise<void> => {
        if (JSON.stringify(snowlumaApp) === JSON.stringify(snowlumaAppPristine)) return;
        await persist(snowlumaApp);
        setSnowlumaAppPristine(snowlumaApp);
    }, [persist, snowlumaApp, snowlumaAppPristine]);

    return {
        snowlumaApp,
        setSnowlumaApp,
        snowlumaAppPristine,
        snowlumaAppLoading,
        snowlumaAppLoadError,
        commitIfDirty,
    };
}

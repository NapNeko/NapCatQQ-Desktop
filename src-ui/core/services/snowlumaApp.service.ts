import { invoke, isTauri } from '../ipc/transport';
import type { SnowLumaAppConfig } from '../ipc/generated/domain/SnowLumaAppConfig';

const defaultConfig = (): SnowLumaAppConfig => ({
    snowlumaWebuiPasswordOverride: '',
    snowlumaWebuiPort: 5099,
    snowlumaRemoteQrLoginEnabled: true,
    snowlumaLocalQrLoginEnabled: true,
});

export const snowlumaAppService = {
    get: async (): Promise<SnowLumaAppConfig> => {
        if (!isTauri) return defaultConfig();
        const config = await invoke<SnowLumaAppConfig>('get_snowluma_app_config');
        return {
            ...config,
            snowlumaRemoteQrLoginEnabled: config.snowlumaRemoteQrLoginEnabled ?? true,
            snowlumaLocalQrLoginEnabled: config.snowlumaLocalQrLoginEnabled ?? true,
        };
    },

    set: async (config: SnowLumaAppConfig): Promise<void> => {
        if (!isTauri) return;
        await invoke<void>('set_snowluma_app_config', { config });
    },
};

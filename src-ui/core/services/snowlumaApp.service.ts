import { invoke, isTauri } from '../ipc/transport';
import type { SnowLumaAppConfig } from '../ipc/generated/domain/SnowLumaAppConfig';

const defaultConfig = (): SnowLumaAppConfig => ({
    snowlumaWebuiPasswordOverride: '',
    snowlumaWebuiPort: 5099,
});

export const snowlumaAppService = {
    get: async (): Promise<SnowLumaAppConfig> => {
        if (!isTauri) return defaultConfig();
        return invoke<SnowLumaAppConfig>('get_snowluma_app_config');
    },

    set: async (config: SnowLumaAppConfig): Promise<void> => {
        if (!isTauri) return;
        await invoke<void>('set_snowluma_app_config', { config });
    },
};
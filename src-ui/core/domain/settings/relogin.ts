// 每份草稿使用独立数组，避免取消对话框时修改已保存配置。
import type { ReloginSettings } from '../../ipc/generated/domain/ReloginSettings';

export function defaultReloginSettings(): ReloginSettings {
    return {
        enabled: false,
        administratorIds: [],
        botIds: [],
        offlineGraceSeconds: 60,
        refreshCooldownSeconds: 30,
    };
}

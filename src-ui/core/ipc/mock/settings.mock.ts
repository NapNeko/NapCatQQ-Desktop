// 浏览器 dev 模式下的后端设置 mock。
// 可变单例：settingsService.set 在非 Tauri 环境写回这里，让 dev 下改设置有连续手感。

import type { BackendSettings } from '../../services/settings.service';

export const mockBackendSettings: BackendSettings = {
    botLoginCheckIntervalMs: 5000,
    performanceMonitorEnabled: true,
    performanceMonitorIntervalMs: 1200,
    githubPat: '',
    closeAction: 'close',
};

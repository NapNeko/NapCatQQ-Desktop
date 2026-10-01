// 调试台入口（侧栏、Bot 卡片按钮、右键菜单）统一问这里。
// 开关落在 app-settings 的 features.apiDebug：后端 DebugManager 也按它起停。

import { useFeatureEnabled } from '../preferences/featureTogglesStore';

export function useDebugConsoleEnabled(): boolean {
    return useFeatureEnabled('apiDebug');
}

// 调试台入口（侧栏、Bot 卡片按钮、右键菜单）统一问这里。
//
// 功能开关（quiet-soaring-otter 的 FeatureToggles.apiDebug）落地后改读 `useFeatureEnabled('apiDebug')`，
// 到时候入口不用再动。

export function useDebugConsoleEnabled(): boolean {
    return true;
}

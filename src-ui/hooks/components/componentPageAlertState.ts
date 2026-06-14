// 组件页告警（清单加载失败 + 单点探测失败）的模块级记忆与抑制状态。
// 跨路由卸载 hook 也不丢；用户手动关闭持久失败条目后写入抑制，直到观测到该 key 对应
// 条件至少恢复一次（非失败状态出现过），才允许再次弹出。

const userDismissedAlertKeys = new Set<string>();

export function isComponentPageAlertSuppressed(key: string): boolean {
    return userDismissedAlertKeys.has(key);
}

export function suppressComponentPageAlert(key: string): void {
    userDismissedAlertKeys.add(key);
}

export function clearComponentPageAlertSuppression(key: string): void {
    userDismissedAlertKeys.delete(key);
}

/** 测试 / dev 重置用。 */
export function _resetComponentPageAlertState(): void {
    userDismissedAlertKeys.clear();
}

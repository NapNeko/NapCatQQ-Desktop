// QQ 系统依赖告警的跨路由抑制状态。
// 用户关闭或开始修复后，在探测确认依赖恢复前不再重复提示。

const suppressedHosts = new Set<string>();

export function isQqDependencyAlertSuppressed(hostId: string): boolean {
    return suppressedHosts.has(hostId);
}

export function suppressQqDependencyAlert(hostId: string): void {
    suppressedHosts.add(hostId);
}

export function clearQqDependencyAlertSuppression(hostId: string): void {
    suppressedHosts.delete(hostId);
}

/** 测试 / dev 重置用。 */
export function _resetQqDependencyAlertState(): void {
    suppressedHosts.clear();
}
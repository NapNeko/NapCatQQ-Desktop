// AppNext 注册 Desktop 协议门禁；Bot 列表等通过 host 复用同一实例，
// 避免页面内再 new 一份 useDesktopConsentGate 导致双弹窗/状态打架。

type PendingAction = () => void | Promise<void>;

type EnsureConsentFn = (action?: PendingAction) => Promise<boolean>;

let ensureConsentFn: EnsureConsentFn | null = null;

export function registerDesktopConsentHost(fn: EnsureConsentFn | null): void {
    ensureConsentFn = fn;
}

/** 若已同意则执行 action；否则走 App 级门禁。返回是否当场已放行。 */
export async function requestDesktopConsent(
    action?: PendingAction,
): Promise<boolean> {
    if (!ensureConsentFn) {
        // 启动极早期：host 尚未注册时不静默放行，避免绕过协议
        throw new Error('用户协议门禁尚未就绪，请稍后再试');
    }
    return ensureConsentFn(action);
}

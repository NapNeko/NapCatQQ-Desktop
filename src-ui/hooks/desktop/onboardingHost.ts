// AppNext 注册引导打开函数；设置·关于通过 request 触发，避免 props 穿透 Settings 树。

type OpenFromSettingsFn = () => void | Promise<void>;

let openFromSettingsFn: OpenFromSettingsFn | null = null;

export function registerOnboardingHost(fn: OpenFromSettingsFn | null): void {
    openFromSettingsFn = fn;
}

export async function requestOnboardingFromSettings(): Promise<void> {
    if (!openFromSettingsFn) {
        throw new Error('入门引导尚未就绪，请稍后再试');
    }
    await openFromSettingsFn();
}

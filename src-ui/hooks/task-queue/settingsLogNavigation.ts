// 设置页打开时若需直达日志 Tab，由 AppNext 写入后在此消费。

let pendingLogTab = false;

export function requestSettingsLogTab(): void {
    pendingLogTab = true;
}

export function consumeSettingsLogTab(): boolean {
    if (!pendingLogTab) return false;
    pendingLogTab = false;
    return true;
}
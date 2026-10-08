// 协议门禁错误判定：后端把「缺 Desktop 协议 / 缺 SnowLuma 上游协议」当控制通道，
// 用错误字符串里的固定标记回传，调用方要区分「门禁触发（去开协议对话框）」和
// 「真失败（弹错误条）」才知道下一步走哪条分支。列表页的启动编排与协议重放
// 都要用到，hooks 层不能反向 import modules，所以判定函数落在这里。

export function isSnowLumaConsentError(err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return (
        message.includes('SNOWLUMA_CONSENT_REQUIRED') || message.includes('"consentRequired":true')
    );
}

export function isDesktopConsentError(err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return message.includes('DESKTOP_CONSENT_REQUIRED');
}

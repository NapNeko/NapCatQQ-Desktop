// 调用没拿到回包时怎么跟用户说，以及 OneBot 回包里 retcode 的人话提示。

import type { DebugCallResponse } from '../../ipc/generated/debug/DebugCallResponse';
import type { DebugError } from '../../ipc/generated/debug/DebugError';
import { NO_CHANNEL_EXITS, UPGRADE_RUNTIME_EXIT, type ChannelExit } from './channelCopy';

interface ErrorCopy {
    title: string;
    detail?: string;
    /** 该不该带一个「去看通道诊断」的入口：鉴权 / 连接 / Bot 状态类问题在通道上能看到原因 */
    channelIssue: boolean;
    /** 「去哪解决」的出口按钮；调试台页面没给 onNavigate 时不画 */
    exits?: ChannelExit[];
}

function seconds(ms: number): string {
    const s = ms / 1000;
    return Number.isInteger(s) ? `${s}` : s.toFixed(1);
}

// 键是后端 DebugError 的 kind：Rust 加一种，这里不补就编译不过
const COPY = {
    bot_not_found: () => ({
        title: '找不到这个 Bot',
        detail: '它可能已被删除，刷新 Bot 列表后重试',
        channelIssue: false,
    }),
    bot_not_running: () => ({
        title: 'Bot 没有在运行',
        detail: '先启动它，再来调用接口',
        channelIssue: true,
    }),
    not_logged_in: () => ({
        title: 'QQ 还没登录',
        detail: '登录成功之后才能调用接口',
        channelIssue: true,
    }),
    channel_unavailable: (e) => ({
        title: '这条通道现在用不了',
        detail: e.reason,
        channelIssue: true,
        exits: NO_CHANNEL_EXITS,
    }),
    upstream_too_old: () => ({
        title: '上游版本太老，没有调试接口',
        detail: '升级到最新版 NapCat / SnowLuma 后可用',
        channelIssue: true,
        exits: [UPGRADE_RUNTIME_EXIT],
    }),
    auth_failed: (e) => ({
        title: '鉴权失败',
        detail: `上游返回 ${e.status}，检查这条通道的 token 是否正确`,
        channelIssue: true,
    }),
    timeout: (e) => ({
        title: '等太久了，调用超时',
        detail: `已等待 ${seconds(e.ms)} 秒。上游可能还在执行，可以调大超时后重试`,
        channelIssue: false,
    }),
    cancelled: () => ({
        title: '已取消',
        detail: '取消只是不再等待回包，上游可能已经执行了',
        channelIssue: false,
    }),
    transport: (e) => ({
        title: '连接出错',
        detail: e.message,
        channelIssue: true,
    }),
    invalid_params: (e) => ({
        title: '参数有问题',
        detail: e.message,
        channelIssue: false,
    }),
    feature_disabled: () => ({
        title: '调试台已关闭',
        detail: '在设置里重新打开后再使用',
        channelIssue: false,
    }),
    internal: (e) => ({
        title: '桌面端内部出错',
        detail: e.message,
        channelIssue: false,
    }),
} satisfies { [K in DebugError['kind']]: (e: Extract<DebugError, { kind: K }>) => ErrorCopy };

export function debugErrorCopy(e: DebugError): { title: string; detail?: string; channelIssue: boolean; exits?: ChannelExit[] } {
    // 表按 kind 索引，这里的类型断言只是因为 TS 没法把「同一个 kind 的键配同一个 kind 的载荷」这层关系带过索引
    const build = COPY[e.kind] as (err: DebugError) => ErrorCopy;
    return build(e);
}

/** OneBot retcode 的常见含义；0 和不认识的返回 null，界面就只显示上游原话 */
export function retcodeHint(retcode: number): string | null {
    switch (retcode) {
        case 1400:
        case 400:
            return '参数不对：缺了必填项，或者类型 / 取值不符合要求';
        case 1401:
            return '权限不足：当前账号或 token 没有调用这个接口的权限';
        case 1403:
            return '被拒绝访问：检查 token 和访问权限，或者上游禁止了这类操作';
        case 1404:
            return '接口不存在：这个 Bot 的版本可能不支持它，或者名字写错了';
        case 1200:
        case 200:
        case 100:
            return '上游执行出错：具体原因看返回里的 message / wording';
        default:
            return null;
    }
}

/**
 * 一次调用失败的一句话原因；成功（拿到回包且 retcode 为 0）返回 null。
 * 没拿到回包用上面的错误文案；拿到了但 OB11 说失败，报 retcode，带上上游的说明。
 */
export function callProblem(res: DebugCallResponse): string | null {
    if (res.result.kind === 'err') {
        const copy = debugErrorCopy(res.result.error);
        return copy.detail ? `${copy.title}：${copy.detail}` : copy.title;
    }
    const o = res.result.outcome;
    if (o.ok) return null;
    const why = o.wording || o.message || retcodeHint(o.retcode) || '';
    return `retcode ${o.retcode}${why ? ` · ${why}` : ''}`;
}

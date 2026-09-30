// 每个标签「按哪个动作填过初始参数」和「那份初始参数长什么样」，放在模块里：切标签、切路由回来都还在。
//
// 目录 / 命令面板打开动作时只给 `{}`，说明读到后中栏才按说明填上（有示例用示例，没有就列出必填项）；
// 填过一次就不再动，用户清空了也不会被重新填回去。「用它新开 get_xxx」这类带着参数打开的标签
// 在打开时就记成填过了，说明读到后不会被当成「还没填」。
//
// 「参数改没改过」的基准也是这份文本，而且只认「当时真正填进 params_text 的那一份」（M2）：
// 换 Bot 后两边说明的初始参数不同（NC 的 id 占位是 ""、SL 不填），按新说明现场重算会把
// 没动过的标签标成改过。所以基准只在播种那一刻记，之后换 Bot、说明变了都不重记，
// store 里打开时记下的 pristine 跟它就是同一份，两套判断不会再打架。

import { initialParamsText } from '../../../core/domain/debug/paramsText';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';

const seeded = new Map<string, string>();
const initialTexts = new Map<string, string>();

export function markSeeded(tabId: string, action: string): void {
    seeded.set(tabId, action);
}

export function wasSeeded(tabId: string, action: string): boolean {
    return seeded.get(tabId) === action;
}

export function rememberInitialText(tabId: string, text: string): void {
    initialTexts.set(tabId, text);
}

/**
 * 拿来判断「参数改没改过」的初始文本。优先拿这个标签真正被填进去的那份（填过才记，
 * 之后不随 Bot / 说明变，基准才稳）；没记过才按当前说明算——那会儿文本多半还是 `{}`，
 * 反正不算改过。一次都没读到过而且还在读，返回 null（不知道）；不会再去读了
 * （没填接口名、没选 Bot、读失败、目录外的接口）按 `{}` 算。
 * `spec` 传 react-query 的原始 data：undefined 表示还没有结果，null 表示目录里没有这个接口。
 */
export function resolveInitialText(tabId: string, spec: DebugActionSpec | null | undefined, loading: boolean): string | null {
    const known = initialTexts.get(tabId);
    if (known !== undefined) return known;
    if (spec !== undefined) return initialParamsText(spec);
    return loading ? null : '{}';
}

/** 测试用 */
export function _resetSeedStateForTests(): void {
    seeded.clear();
    initialTexts.clear();
}

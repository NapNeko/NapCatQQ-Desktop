// 配置读写命令的结构化错误识别。
// 后端 `read/write_app_*_config*` 失败时 reject 的是 `AppConfigError { kind, message, issues }`，
// 其它命令仍是裸字符串；这里把两种形状统一成可分流的对象。

import type { AppConfigError, AppConfigErrorKind } from '../../ipc/types';
import { errorText } from '../errors';

// 按生成的联合类型逐个列：Rust 那边加了一种 kind 而这里没跟上，typecheck 就过不去，
// 不会让新 kind 的错误被当成裸字符串归进 other
const KIND_TABLE = {
    conflict: true,
    invalid: true,
    unsupported: true,
    not_running: true,
    auth: true,
    unreachable: true,
    other: true,
} satisfies Record<AppConfigErrorKind, true>;

const KINDS: ReadonlySet<string> = new Set(Object.keys(KIND_TABLE));

export function isAppConfigError(err: unknown): err is AppConfigError {
    if (!err || typeof err !== 'object') return false;
    const e = err as Partial<AppConfigError>;
    return typeof e.kind === 'string' && KINDS.has(e.kind) && typeof e.message === 'string';
}

/** 任意错误 → AppConfigError（非结构化的一律归 other，issues 为空） */
export function toAppConfigError(err: unknown): AppConfigError {
    if (isAppConfigError(err)) return { ...err, issues: err.issues ?? [] };
    return { kind: 'other', message: errorText(err), issues: [] };
}

export function makeAppConfigError(
    kind: AppConfigErrorKind,
    message: string,
    issues: AppConfigError['issues'] = [],
): AppConfigError {
    return { kind, message, issues };
}

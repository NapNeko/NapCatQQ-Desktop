// 配置读写命令的结构化错误识别。
// 后端 `read/write_app_*_config*` 失败时 reject 的是 `AppConfigError { kind, message, issues }`，
// 其它命令仍是裸字符串；这里把两种形状统一成可分流的对象。

import type { AppConfigError, AppConfigErrorKind } from '../../ipc/types';
import { errorText } from '../errors';

const KINDS: ReadonlySet<string> = new Set<AppConfigErrorKind>([
    'conflict',
    'invalid',
    'unsupported',
    'other',
]);

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

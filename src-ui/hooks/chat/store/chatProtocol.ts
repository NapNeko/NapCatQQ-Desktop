import { debugErrorCopy } from '../../../core/domain/debug/errorCopy';
import type { DebugCallResponse } from '../../../core/ipc/generated/debug/DebugCallResponse';

export function dataOf(response: DebugCallResponse): unknown {
    if (response.result.kind === 'err') {
        const copy = debugErrorCopy(response.result.error);
        throw new Error([copy.title, copy.detail].filter(Boolean).join('：'));
    }
    const result = response.result.outcome;
    if (!result.ok)
        throw new Error(result.wording || result.message || `请求失败（${result.retcode}）`);
    if (result.truncated) throw new Error('返回内容过大，请缩小范围后重试');
    return result.data;
}

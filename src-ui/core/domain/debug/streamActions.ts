// 流式接口（分块上传 / 下载）的小组装：固定的动作名、本机文件占位标记、进度文案。
// 协议面在后端 `crates/ncd-runtime/src/onebot_debug/stream.rs`，两边用同一批字面量，
// 不靠目录判断（目录合出来之前这些判断就要成立）。

import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugLocalFile } from '../../ipc/generated/debug/DebugLocalFile';
import type { DebugStreamProgress } from '../../ipc/generated/debug/DebugStreamProgress';

/** 分块协议的上传动作名 */
export const STREAM_UPLOAD_ACTION = 'upload_file_stream';
/** 「一次请求、多帧回答」的下载动作；`clean_stream_temp_file` 是普通单帧调用，不在列 */
export const STREAM_DOWNLOAD_ACTIONS: ReadonlySet<string> = new Set([
    'download_file_stream',
    'download_file_image_stream',
    'download_file_record_stream',
    'test_download_stream',
]);
/**
 * 参数里「这个字符串值要用本机文件替换」的占位前缀，后面直接跟本机绝对路径。
 * 与后端 `LOCAL_FILE_TOKEN_PREFIX` 同一字面量
 */
export const LOCAL_FILE_PREFIX = 'ncd-local-file://';

/** 点了名不是内部通道也不是「自动」时，分块传输的固定路销。与后端那条理由同一句 */
export const STREAM_CHANNEL_REASON = '该通道不支持流式接口；分块传输只在内部通道上支持';

export const isLocalFileToken = (v: unknown): v is string =>
    typeof v === 'string' && v.startsWith(LOCAL_FILE_PREFIX) && v.length > LOCAL_FILE_PREFIX.length;

export const localFileTokenFor = (path: string): string => `${LOCAL_FILE_PREFIX}${path}`;

/** 递归找参数里所有本机文件占位（同一个文件出现在多处只算一份） */
export function localFilesInParams(params: unknown): DebugLocalFile[] {
    const out: DebugLocalFile[] = [];
    const seen = new Set<string>();
    const walk = (v: unknown): void => {
        if (isLocalFileToken(v)) {
            if (!seen.has(v)) {
                seen.add(v);
                out.push({ path: v.slice(LOCAL_FILE_PREFIX.length), token: v });
            }
            return;
        }
        if (Array.isArray(v)) return v.forEach(walk);
        if (typeof v === 'object' && v !== null) Object.values(v).forEach(walk);
    };
    walk(params);
    return out;
}

/** 这次调用要不要走流式命令：下载动作、或参数里有本机文件占位 */
export function needsStreamCall(action: string, params: unknown): boolean {
    return STREAM_DOWNLOAD_ACTIONS.has(action) || localFilesInParams(params).length > 0;
}

/**
 * 发送前在前端就能确定的「不行」：分块下载、或带本机文件的分块上传，点名的 HTTP / WS 通道。
 * 「自动」和内部通道放行给后端落；其它动作带本机文件时传输走内部通道、调用照样走所选通道
 */
export function streamChannelBlocker(
    action: string,
    localFiles: number,
    channel: DebugChannelId,
): string | null {
    if (channel.kind === 'internal' || channel.kind === 'auto') return null;
    if (STREAM_DOWNLOAD_ACTIONS.has(action)) return STREAM_CHANNEL_REASON;
    if (action === STREAM_UPLOAD_ACTION && localFiles > 0) return STREAM_CHANNEL_REASON;
    return null;
}

// ---------------------------------------------------------------------------
// 进度文案
// ---------------------------------------------------------------------------

/** 字节数的人的读法：100 B、42.0 KiB、12.4 MiB、1.2 GiB */
export function bytesText(n: number): string {
    if (n < 1024) return `${n} B`;
    const units = ['KiB', 'MiB', 'GiB'] as const;
    let v = n / 1024;
    let i = 0;
    while (v >= 1024 && i < units.length - 1) {
        v /= 1024;
        i += 1;
    }
    return `${v.toFixed(1)} ${units[i]}`;
}

/** 进度那行字：总量已知是「上传中 12.4 / 48.0 MiB（26%）」，未知只写已传量 */
export function progressText(p: DebugStreamProgress): string {
    if (p.stage === 'reading') return `读本机文件 ${p.file_name}…`;
    if (p.stage === 'calling') return '正在调用…';
    const label = p.stage === 'uploading' ? '上传中' : '下载中';
    const total = p.total_bytes;
    if (total && total > 0) {
        const pct = Math.min(100, Math.floor((p.done_bytes / total) * 100));
        return `${label} ${bytesText(p.done_bytes)} / ${bytesText(total)}（${pct}%）`;
    }
    const chunks = p.done_chunks > 0 ? `，${p.done_chunks} 块` : '';
    return `${label} ${bytesText(p.done_bytes)}${chunks}`;
}

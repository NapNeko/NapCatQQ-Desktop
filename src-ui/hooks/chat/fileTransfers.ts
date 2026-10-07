// 聊天里的文件传输。放在模块里而不是组件里：关掉群文件对话框下载照样继续，
// 时间线文件卡片和对话框看到的是同一份进度。
import { useSyncExternalStore } from 'react';
import {
    chatGroupFilesService,
    type LocalUpload,
} from '../../core/services/chat-group-files.service';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugStreamProgress } from '../../core/ipc/generated/debug/DebugStreamProgress';
import type { ChatFileSource } from '../../core/ipc/generated/chat/ChatFileSource';
import { accountKey } from '../../core/domain/chat/model';

export type TransferState = 'running' | 'done' | 'failed' | 'cancelled';
export interface FileTransfer {
    key: string;
    requestId: string;
    kind: 'download' | 'upload';
    scope: string;
    name: string;
    state: TransferState;
    progress?: DebugStreamProgress;
    /** 下载完成后的本机路径 */
    path?: string;
    error?: string;
    groupId?: string;
    folderId?: string;
}

const transfers = new Map<string, FileTransfer>();
const listeners = new Set<() => void>();
let snapshot: FileTransfer[] = [];
// 已完成 / 失败的只留最近这些，免得长时间挂着越攒越多
const KEPT_FINISHED = 40;

function emit() {
    const finished = [...transfers.values()].filter((t) => t.state !== 'running');
    for (const old of finished.slice(0, Math.max(0, finished.length - KEPT_FINISHED)))
        transfers.delete(old.key);
    snapshot = [...transfers.values()];
    for (const listener of listeners) listener();
}
function patch(key: string, requestId: string, next: Partial<FileTransfer>) {
    const current = transfers.get(key);
    // 同一个键被重新发起过，迟到的旧结果不能盖掉新的
    if (!current || current.requestId !== requestId) return;
    transfers.set(key, { ...current, ...next });
    emit();
}
const subscribe = (listener: () => void) => {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
};
const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));
const isCancel = (message: string) => /已取消|cancel/i.test(message);

export const scopeOf = (target: DebugTarget) => accountKey(target.bot_id, String(target.qq_id));
export const downloadKey = (scope: string, source: ChatFileSource) =>
    `${scope}:download:${source.kind}:${source.kind === 'group' ? source.groupId : source.userId}:${source.fileId}`;

export function useFileTransfers(): FileTransfer[] {
    return useSyncExternalStore(subscribe, () => snapshot);
}
export function useFileTransfer(key: string | null): FileTransfer | undefined {
    return useSyncExternalStore(subscribe, () => (key ? transfers.get(key) : undefined));
}

export async function startDownload(
    target: DebugTarget,
    source: ChatFileSource,
    name: string,
): Promise<void> {
    const scope = scopeOf(target);
    const key = downloadKey(scope, source);
    if (transfers.get(key)?.state === 'running') return;
    const requestId = crypto.randomUUID();
    transfers.set(key, {
        key,
        requestId,
        kind: 'download',
        scope,
        name,
        state: 'running',
        groupId: source.kind === 'group' ? source.groupId : undefined,
    });
    emit();
    try {
        const path = await chatGroupFilesService.download(
            target,
            source,
            name,
            requestId,
            (progress) => patch(key, requestId, { progress }),
        );
        if (path === null) {
            // 另存为对话框点了取消
            if (transfers.get(key)?.requestId === requestId) {
                transfers.delete(key);
                emit();
            }
            return;
        }
        patch(key, requestId, { state: 'done', path });
    } catch (error) {
        const message = errorText(error);
        patch(key, requestId, {
            state: isCancel(message) ? 'cancelled' : 'failed',
            error: message,
        });
    }
}

export async function startUpload(
    target: DebugTarget,
    groupId: string,
    folderId: string,
    file: LocalUpload,
    onDone: () => void,
): Promise<void> {
    const requestId = crypto.randomUUID();
    const scope = scopeOf(target);
    const key = `${scope}:upload:${requestId}`;
    transfers.set(key, {
        key,
        requestId,
        kind: 'upload',
        scope,
        name: file.name,
        state: 'running',
        groupId,
        folderId,
    });
    emit();
    try {
        await chatGroupFilesService.upload(target, groupId, folderId, file, requestId, (progress) =>
            patch(key, requestId, { progress }),
        );
        patch(key, requestId, { state: 'done' });
        onDone();
    } catch (error) {
        const message = errorText(error);
        patch(key, requestId, {
            state: isCancel(message) ? 'cancelled' : 'failed',
            error: message,
        });
    }
}

export function cancelTransfer(key: string): void {
    const transfer = transfers.get(key);
    if (transfer?.state === 'running')
        void chatGroupFilesService.cancel(transfer.requestId).catch(() => {});
}
export function dismissTransfer(key: string): void {
    if (transfers.get(key)?.state === 'running') return;
    transfers.delete(key);
    emit();
}
export function openDownload(transfer: FileTransfer, reveal: boolean): Promise<void> {
    if (!transfer.path) return Promise.resolve();
    return chatGroupFilesService.open(transfer.path, reveal);
}

// 存储损坏提示：三份存储文件里任何一份读不出来，就在这里说一次「从空白开始、原文件挪去了哪」。
import { useState } from 'react';
import { AlertTriangle, X } from 'lucide-react';
import { useDebugWorkspaceSelector } from '../../../hooks/debug/debugWorkspaceStore';
import { useDebugStorageNotices } from '../../../hooks/debug/useDebugStorageNotices';
import type { DebugStorageNotice } from '../../../core/ipc/generated/debug/DebugStorageNotice';

const STORAGE_FILE_LABEL: Record<string, string> = {
    'workspace.json': '工作区（标签页和草稿）',
    'collections.json': '收藏',
    'history.jsonl': '调用历史',
};

// 提示看过、关掉了，这次运行里就别每进一次页面弹一次
let storageNoticesDismissed = false;

export function StorageNotices() {
    // 等工作区读完再取：那次加载里三份文件一起读过，损坏探测的结果已经齐了，
    // 不然这里会拿到加载前的空提示，而且 staleTime: Infinity 以后也不会再问
    const loaded = useDebugWorkspaceSelector((s) => s.loaded);
    const notices = useDebugStorageNotices({ enabled: loaded }).data;
    const [dismissed, setDismissed] = useState(storageNoticesDismissed);
    if (dismissed || !notices || notices.length === 0) return null;
    const close = () => {
        storageNoticesDismissed = true;
        setDismissed(true);
    };
    return (
        <div
            role="status"
            className="flex shrink-0 items-start gap-2.5 rounded-md border border-warning/30 bg-warning-soft/60 px-3 py-2"
        >
            <AlertTriangle
                size={14}
                strokeWidth={2.2}
                aria-hidden
                className="mt-0.5 shrink-0 text-warning"
            />
            <div className="min-w-0 flex-1 space-y-0.5 text-xs text-text-secondary">
                {notices.map((n: DebugStorageNotice) => (
                    <p key={n.file} className="break-words">
                        {STORAGE_FILE_LABEL[n.file] ?? n.file}读不出来（{n.reason}
                        ），这次从空白开始；原文件挪到了
                        <span className="mx-1 font-mono text-[11px] text-text">{n.moved_to}</span>
                    </p>
                ))}
            </div>
            <button
                type="button"
                onClick={close}
                aria-label="关闭提示"
                className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-xs text-text-tertiary transition-colors hover:bg-warning-soft hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            >
                <X size={13} aria-hidden />
            </button>
        </div>
    );
}

// 时间线文件卡片里的下载入口，和群文件对话框共用一份传输状态。
import { ArrowDownToLine } from 'lucide-react';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { Contact } from '../../../core/domain/chat/model';
import { fileSegmentName, fileSegmentSource } from '../../../core/domain/chat/groupFiles';
import {
    downloadKey,
    scopeOf,
    startDownload,
    useFileTransfer,
} from '../../../hooks/chat/fileTransfers';
import { TransferLine } from './groupFileParts';
import './group-files.css';

export function ChatFileAction({
    data,
    target,
    contact,
}: {
    data: Record<string, unknown>;
    target: DebugTarget;
    contact: Contact;
}) {
    const source = fileSegmentSource(data, contact);
    const transfer = useFileTransfer(source ? downloadKey(scopeOf(target), source) : null);
    if (!source) return null;
    const name = fileSegmentName(data);
    const start = () => void startDownload(target, source, name);
    if (transfer)
        return (
            <span className="native-chat-file-action">
                <TransferLine transfer={transfer} onRetry={start} compact />
            </span>
        );
    return (
        <span className="native-chat-file-action">
            <button type="button" onClick={start}>
                <ArrowDownToLine size={12} aria-hidden />
                下载
            </button>
        </span>
    );
}

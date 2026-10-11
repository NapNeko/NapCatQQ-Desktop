// 文件段：文件名 + 大小，具体动作（下载 / 打开）由宿主注入。

import {
    FileArchive,
    FileAudio,
    FileCode2,
    FileImage,
    FileSpreadsheet,
    FileText,
    FileVideo,
    File,
    Package,
    Presentation,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { fileKind, type FileKind } from '../../../core/domain/chat/groupFiles';
import { fileSizeLabel } from '../../../core/domain/debug/chatFormat';
import { useChatView } from '../chatContext';
import { str } from './model';
import './fileCard.css';

const ICONS: Record<FileKind, LucideIcon> = {
    archive: FileArchive,
    image: FileImage,
    audio: FileAudio,
    video: FileVideo,
    code: FileCode2,
    doc: FileText,
    sheet: FileSpreadsheet,
    slide: Presentation,
    app: Package,
    other: File,
};

export function FileCard({ data }: { data: Record<string, unknown> }) {
    const name = str(data.name) || str(data.file_name) || str(data.file) || '文件';
    const size = fileSizeLabel(data.file_size ?? data.size);
    const { fileAction } = useChatView();
    const extension = name
        .trim()
        .match(/\.([a-z][a-z0-9]{0,9})$/i)?.[1]
        .toUpperCase();
    const category = fileKind(name.trim());
    const Icon = ICONS[category];
    return (
        <span
            className="chat-file-card"
            data-category={category}
            data-extension={extension}
            role="group"
            aria-label={name}
        >
            <span className="chat-file-card-icon">
                <Icon size={24} strokeWidth={1.6} aria-hidden />
            </span>
            <span className="chat-file-card-content">
                <span className="chat-file-card-name" title={name}>
                    {name}
                </span>
                <span className="chat-file-card-meta">
                    {extension || '文件'}
                    {size ? ` · ${size}` : ''}
                </span>
                {fileAction && <span className="chat-file-card-actions">{fileAction(data)}</span>}
            </span>
        </span>
    );
}

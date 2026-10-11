// 文件段：文件名 + 大小，具体动作（下载 / 打开）由宿主注入。

import {
    FileArchive,
    FileAudio,
    FileCode2,
    FileImage,
    FileText,
    FileVideo,
    File,
} from 'lucide-react';
import { fileSizeLabel } from '../../../core/domain/debug/chatFormat';
import { useChatView } from '../chatContext';
import { str } from './model';
import './fileCard.css';

export function FileCard({ data }: { data: Record<string, unknown> }) {
    const name = str(data.name) || str(data.file_name) || str(data.file) || '文件';
    const size = fileSizeLabel(data.file_size ?? data.size);
    const { fileAction } = useChatView();
    const extension = name.split('.').pop()?.toLowerCase() ?? '';
    const category = /^(zip|rar|7z|tar|gz)$/.test(extension)
        ? 'archive'
        : /^(png|jpe?g|gif|webp|bmp|svg)$/.test(extension)
          ? 'image'
          : /^(mp3|wav|ogg|flac|m4a|silk)$/.test(extension)
            ? 'audio'
            : /^(mp4|mkv|mov|webm|avi)$/.test(extension)
              ? 'video'
              : /^(json|js|ts|py|rs|html|css|xml|yaml|yml)$/.test(extension)
                ? 'code'
                : /^(pdf|docx?|xlsx?|pptx?|txt|md)$/.test(extension)
                  ? 'document'
                  : 'file';
    const Icon = {
        archive: FileArchive,
        image: FileImage,
        audio: FileAudio,
        video: FileVideo,
        code: FileCode2,
        document: FileText,
        file: File,
    }[category];
    return (
        <span className="chat-file-card" data-category={category}>
            <span className="chat-file-card-icon">
                <Icon size={24} strokeWidth={1.6} aria-hidden />
            </span>
            <span className="chat-file-card-content">
                <span className="chat-file-card-name" title={name}>
                    {name}
                </span>
                <span className="chat-file-card-meta">
                    {extension && extension.length <= 10 ? extension.toUpperCase() : '文件'}
                    {size ? ` · ${size}` : ''}
                </span>
                {fileAction && <span className="chat-file-card-actions">{fileAction(data)}</span>}
            </span>
        </span>
    );
}

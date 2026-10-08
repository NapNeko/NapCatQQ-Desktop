// 文件段：文件名 + 大小，具体动作（下载 / 打开）由宿主注入。

import { FileText } from 'lucide-react';
import { fileSizeLabel } from '../../../core/domain/debug/chatFormat';
import { useChatView } from '../chatContext';
import { str } from './model';
import { Card } from './card';

export function FileCard({ data }: { data: Record<string, unknown> }) {
    const name = str(data.name) || str(data.file_name) || str(data.file) || '文件';
    const size = fileSizeLabel(data.file_size ?? data.size);
    const { fileAction } = useChatView();
    return (
        <Card icon={<FileText size={16} aria-hidden />} title={name} sub={size || '文件'}>
            {fileAction?.(data)}
        </Card>
    );
}

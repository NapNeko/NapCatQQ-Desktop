// 收藏面板顶部工具条：数量文案 + 新建文件夹 / 导入 / 导出。

import { FileInput, FileOutput, FolderPlus } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import type { CollectionsView } from '../../../core/domain/debug/collectionsOps';
import { COLUMN_HEADER_CLASS } from '../ColumnFrame';
import { IconAction } from './panelParts';

export interface CollectionsHeaderProps {
    view: CollectionsView | null;
    total: number;
    onNewFolder: () => void;
    importPending: boolean;
    onImport: () => void;
    exportPending: boolean;
    onExport: () => void;
}

export function CollectionsHeader({
    view,
    total,
    onNewFolder,
    importPending,
    onImport,
    exportPending,
    onExport,
}: CollectionsHeaderProps) {
    return (
        <div className={cn(COLUMN_HEADER_CLASS, 'gap-1 pl-3')}>
            <span className="min-w-0 flex-1 truncate text-[11px] text-text-tertiary">
                {view ? (total > 0 ? `${total} 个请求` : '收藏') : '收藏'}
                {view && view.folders.length > 0 ? ` · ${view.folders.length} 个文件夹` : ''}
            </span>
            <IconAction
                icon={FolderPlus}
                label="新建文件夹"
                size="md"
                tooltipSide="bottom"
                disabledReason={view ? null : '收藏还没读出来'}
                onClick={onNewFolder}
            />
            <IconAction
                icon={FileInput}
                label="导入收藏"
                size="md"
                tooltipSide="bottom"
                busy={importPending}
                onClick={onImport}
            />
            <IconAction
                icon={FileOutput}
                label="导出收藏"
                size="md"
                tooltipSide="bottom"
                busy={exportPending}
                disabledReason={
                    total === 0 && (view?.folders.length ?? 0) === 0 ? '还没有收藏' : null
                }
                onClick={onExport}
            />
        </div>
    );
}

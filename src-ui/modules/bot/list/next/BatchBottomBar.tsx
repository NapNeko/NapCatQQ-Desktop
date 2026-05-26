// 批量模式专属底部命令栏（chip 风格悬浮）。
//
// 替换旧 Fluent BatchToolbar：保留旧 Python 版"底部居中浮动 chip"的视觉形态，
// 但改用 Tailwind v4 暖粉桃色 + Radix 风格按钮组。
//
// 视觉差异于旧 Fluent 版：
//   - 不在 selectedCount === 0 时整个隐藏；改成 disabled 状态保留 chip，让用户
//     看到"批量模式开了" + "需要先选东西"两层信息，避免空模式下 chip 闪现闪灭。
//   - 加左侧 "已选 N / M" 计数 + "全选 / 取消全选" 两按钮（旧 Fluent 版没做，
//     旧 Python 版有，对齐回去）。

import { CheckCheck, Play, Square, Trash2, X } from 'lucide-react';
import { Button } from '../../../../shared/ui';

interface BatchBottomBarProps {
    /** 已选中数。 */
    selectedCount: number;
    /** 当前列表总数（用来显示 "N / M" 文案）。 */
    totalCount: number;
    /** 是否所有项都选中（驱动"全选 / 取消全选"两个按钮的状态）。 */
    allSelected: boolean;
    onSelectAll: () => void;
    onSelectNone: () => void;
    onBatchStart: () => void;
    onBatchStop: () => void;
    onBatchDelete: () => void;
    onExitBatch: () => void;
    /** 后端 mutation 进行中：禁用所有动作，避免重复触发。 */
    busy?: boolean;
}

export function BatchBottomBar({
    selectedCount,
    totalCount,
    allSelected,
    onSelectAll,
    onSelectNone,
    onBatchStart,
    onBatchStop,
    onBatchDelete,
    onExitBatch,
    busy = false,
}: BatchBottomBarProps) {
    const hasSelection = selectedCount > 0;

    return (
        <div className="pointer-events-none fixed bottom-6 left-1/2 z-30 -translate-x-1/2">
            <div
                className="
                    pointer-events-auto flex items-center gap-2 rounded-full
                    bg-elevated px-4 py-2 ring-1 ring-border-subtle shadow-popover
                    backdrop-blur-sm
                "
                role="toolbar"
                aria-label="批量管理"
            >
                {/* 已选计数 */}
                <span className="select-none text-xs font-medium tabular-nums text-text-secondary">
                    已选 <span className="font-semibold text-text">{selectedCount}</span> / {totalCount}
                </span>

                <Divider />

                {/* 全选 / 取消全选 */}
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={allSelected ? onSelectNone : onSelectAll}
                    disabled={busy || totalCount === 0}
                >
                    <CheckCheck size={14} strokeWidth={2.2} />
                    {allSelected ? '取消全选' : '全选'}
                </Button>

                <Divider />

                {/* 启动 / 停止 / 删除 */}
                <Button
                    size="sm"
                    variant="primary"
                    onClick={onBatchStart}
                    disabled={busy || !hasSelection}
                >
                    <Play size={14} strokeWidth={2.4} />
                    启动
                </Button>
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={onBatchStop}
                    disabled={busy || !hasSelection}
                >
                    <Square size={14} strokeWidth={2.4} />
                    停止
                </Button>
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={onBatchDelete}
                    disabled={busy || !hasSelection}
                    className="text-danger hover:bg-danger-soft hover:text-danger"
                >
                    <Trash2 size={14} strokeWidth={2.2} />
                    删除
                </Button>

                <Divider />

                {/* 退出 */}
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={onExitBatch}
                    disabled={busy}
                >
                    <X size={14} strokeWidth={2.2} />
                    退出
                </Button>
            </div>
        </div>
    );
}

function Divider() {
    return <span aria-hidden className="mx-1 h-4 w-px bg-border-subtle" />;
}

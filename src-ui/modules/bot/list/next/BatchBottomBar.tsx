// 批量模式专属底部命令栏(chip 风格悬浮)。GSAP 版。
//
// 替换旧 Fluent BatchToolbar:保留旧 Python 版"底部居中浮动 chip"的视觉形态,
// 但改用 Tailwind v4 暖粉桃色 + Radix 风格按钮组。
//
// 视觉差异于旧 Fluent 版:
//   - 不在 selectedCount === 0 时整个隐藏;改成 disabled 状态保留 chip
//   - 加左侧 "已选 N / M" 计数 + "全选 / 取消全选" 两按钮
//
// 互斥:批量模式 visible=false 时跑 fly-out exit。

import { CheckCheck, Play, Square, Trash2, X } from 'lucide-react';
import { forwardRef } from 'react';
import gsap from 'gsap';
import { Button } from '../../../../shared/ui';
import { Counter, GsapPresence, type EnterFn, type ExitFn } from '../../../../shared/ui/motion';

interface BatchBottomBarProps {
    visible: boolean;
    selectedCount: number;
    totalCount: number;
    allSelected: boolean;
    onSelectAll: () => void;
    onSelectNone: () => void;
    onBatchStart: () => void;
    onBatchStop: () => void;
    onBatchDelete: () => void;
    onExitBatch: () => void;
    busy?: boolean;
}

const enter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, y: 24, scale: 0.94 },
        {
            autoAlpha: 1,
            y: 0,
            scale: 1,
            duration: env.duration('base'),
            ease: env.ease.release,
        },
    );

const exit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        y: 24,
        scale: 0.94,
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });

export function BatchBottomBar({
    visible,
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

    // 双层容器:外层 fixed + 居中(GSAP 不动这层 transform),
    // 内层 GsapPresence 自由动 x/y/scale 不影响居中。
    return (
        <div className="pointer-events-none fixed bottom-6 left-1/2 z-30 -translate-x-1/2">
            <GsapPresence visible={visible} onEnter={enter} onExit={exit}>
                <BarBody>
                    <div
                        className="
                            pointer-events-auto flex items-center gap-2 rounded-full
                            bg-elevated px-4 py-2 ring-1 ring-border-subtle shadow-popover
                            backdrop-blur-sm whitespace-nowrap
                        "
                        role="toolbar"
                        aria-label="批量管理"
                    >
                        <span className="select-none text-xs font-medium tabular-nums text-text-secondary">
                            已选{' '}
                            <Counter value={selectedCount} className="font-semibold text-text" />
                            {' '}/ {totalCount}
                        </span>

                        <Divider />

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
                </BarBody>
            </GsapPresence>
        </div>
    );
}

const BarBody = forwardRef<HTMLDivElement, { children: React.ReactNode }>(
    ({ children }, ref) => (
        <div
            ref={ref}
            // 内层只让 GSAP 自由动 transform / autoAlpha,不再写 fixed 定位 +
            // -translate,免得跟 GSAP 的 transform 冲突。
            style={{ visibility: 'hidden', opacity: 0 }}
        >
            {children}
        </div>
    ),
);
BarBody.displayName = 'BarBody';

function Divider() {
    return <span aria-hidden className="mx-1 h-4 w-px bg-border-subtle" />;
}

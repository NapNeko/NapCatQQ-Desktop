// 主机切换条：组件页顶部一排可点的主机标签，一次只看一台机器的组件。
//
// 旧版把每台机器塞成一张大卡上下堆叠，机器一多就要滚很久、还难比较。改成
// 切换条 + 单机视图：默认进来停在第一台（本机），点别的标签切过去。每个标签
// 带连接点 + 已装/总数小计，不点开也能扫一眼哪台装得多。
//
// 只有一台机器时整条不渲染（没切换可言），由调用方决定。

import React from 'react';
import { cn } from '../../shared/utils/cn';
import type { MachineView } from '../../core/domain/components/types';
import { machineSummary } from '../../core/domain/components/types';

interface HostSwitcherProps {
    machines: MachineView[];
    activeHostId: string;
    onSelect: (hostId: string) => void;
}

export const HostSwitcher: React.FC<HostSwitcherProps> = ({ machines, activeHostId, onSelect }) => (
    <div
        role="tablist"
        aria-label="选择主机"
        className="flex shrink-0 flex-wrap items-center gap-2 pb-1"
    >
        {machines.map((machine) => (
            <HostTab
                key={machine.host.host_id}
                machine={machine}
                active={machine.host.host_id === activeHostId}
                onSelect={() => onSelect(machine.host.host_id)}
            />
        ))}
    </div>
);

const HostTab: React.FC<{
    machine: MachineView;
    active: boolean;
    onSelect: () => void;
}> = ({ machine, active, onSelect }) => {
    const { host } = machine;
    const isRemote = host.locality === 'remote';
    const { installed, total } = machineSummary(machine);

    return (
        <button
            type="button"
            role="tab"
            aria-selected={active}
            onClick={onSelect}
            className={cn(
                'group flex flex-col gap-0.5 rounded-md border px-3 py-2 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                active
                    ? 'border-brand/40 bg-brand-soft'
                    : 'border-border-subtle bg-inset/40 hover:bg-inset/70',
            )}
        >
            {/* 圆点和主标题放同一 items-center 行,圆点严格对齐名字中线 —— 不再
                跟"两行文本块"的中点对齐,避免中文字形偏上导致的错位观感。 */}
            <span className="flex items-center gap-2">
                <span
                    aria-hidden
                    className={cn(
                        'inline-block h-2 w-2 shrink-0 rounded-full',
                        isRemote ? 'bg-success shadow-glow-success' : 'bg-brand',
                    )}
                />
                <span
                    className={cn(
                        'text-[13px] font-medium leading-tight',
                        active ? 'text-text' : 'text-text-secondary group-hover:text-text',
                    )}
                >
                    {host.display_name}
                </span>
            </span>
            {/* 副标题缩进对齐到主标题左缘(圆点 8px + gap 8px = ml-4)。 */}
            <span className="ml-4 text-[10px] uppercase leading-tight tracking-wider text-text-tertiary">
                {host.os} · {isRemote ? '远端' : '本机'} · {installed}/{total}
            </span>
        </button>
    );
};

export default HostSwitcher;

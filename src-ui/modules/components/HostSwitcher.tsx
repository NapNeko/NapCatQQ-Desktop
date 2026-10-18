// 主机切换条：顶部可点击的分段主机标签栏，一次只看一台机器的组件。

import React from 'react';
import { Monitor, Server } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import type { MachineView } from '../../core/domain/components/types';
import { machineSummary } from '../../core/domain/components/types';

interface HostSwitcherProps {
    machines: MachineView[];
    activeHostId: string;
    onSelect: (hostId: string) => void;
}

export const HostSwitcher: React.FC<HostSwitcherProps> = ({ machines, activeHostId, onSelect }) => {
    const tabRefs = React.useRef<Record<string, HTMLButtonElement | null>>({});

    React.useEffect(() => {
        tabRefs.current[activeHostId]?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
    }, [activeHostId]);

    const focusHost = (hostId: string) => {
        queueMicrotask(() => tabRefs.current[hostId]?.focus());
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
        const target = (e.target as HTMLElement).closest<HTMLButtonElement>(
            '[role="tab"][data-host-id]',
        );
        if (!target) return;

        const currentIndex = machines.findIndex(
            (machine) => machine.host.host_id === target.dataset.hostId,
        );
        if (currentIndex < 0) return;

        let nextIndex: number | null;
        switch (e.key) {
            case 'ArrowRight':
            case 'ArrowDown':
                nextIndex = (currentIndex + 1) % machines.length;
                break;
            case 'ArrowLeft':
            case 'ArrowUp':
                nextIndex = (currentIndex - 1 + machines.length) % machines.length;
                break;
            case 'Home':
                nextIndex = 0;
                break;
            case 'End':
                nextIndex = machines.length - 1;
                break;
            default:
                return;
        }

        e.preventDefault();
        const nextHostId = machines[nextIndex].host.host_id;
        onSelect(nextHostId);
        focusHost(nextHostId);
    };

    return (
        <div
            role="tablist"
            aria-label="选择主机"
            data-tour-id="host-switcher"
            className="flex w-max max-w-full shrink-0 items-center gap-1 overflow-x-auto rounded-md border border-border-subtle bg-inset/50 p-1"
            onKeyDown={handleKeyDown}
        >
            {machines.map((machine) => (
                <HostTab
                    key={machine.host.host_id}
                    ref={(node) => {
                        tabRefs.current[machine.host.host_id] = node;
                    }}
                    machine={machine}
                    active={machine.host.host_id === activeHostId}
                    onSelect={() => onSelect(machine.host.host_id)}
                />
            ))}
        </div>
    );
};

const HostTab = React.forwardRef<
    HTMLButtonElement,
    {
        machine: MachineView;
        active: boolean;
        onSelect: () => void;
    }
>(({ machine, active, onSelect }, ref) => {
    const { host } = machine;
    const isRemote = host.locality === 'remote';
    const isFailed = host.state === 'failed';
    const { installed, total } = machineSummary(machine);
    const tabId = `component-host-tab-${host.host_id}`;
    const Icon = isRemote ? Server : Monitor;

    return (
        <button
            ref={ref}
            type="button"
            role="tab"
            id={tabId}
            data-host-id={host.host_id}
            data-tour-id={host.host_id === 'tour:demo-remote' ? 'host-tab-demo-remote' : undefined}
            aria-selected={active}
            aria-label={`${host.display_name}，${host.os}，${isRemote ? '远端' : '本机'}${isFailed ? '，连接中断' : ''}，已安装 ${installed} / ${total}`}
            tabIndex={active ? 0 : -1}
            onClick={onSelect}
            className={cn(
                'group flex min-w-0 max-w-full shrink-0 items-center gap-2 rounded-sm border px-2.5 py-1.5 text-left transition-all duration-fast',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                isFailed
                    ? active
                        ? 'border-danger/45 bg-danger/12 text-danger shadow-xs'
                        : 'border-transparent text-danger/85 hover:bg-danger/8'
                    : active
                      ? 'border-border-subtle bg-elevated text-text shadow-xs'
                      : 'border-transparent text-text-secondary hover:bg-surface/60 hover:text-text',
            )}
        >
            <span
                aria-hidden
                className={cn(
                    'inline-block h-1.5 w-1.5 shrink-0 rounded-full',
                    isFailed
                        ? 'bg-danger'
                        : isRemote
                          ? 'bg-success shadow-glow-success'
                          : 'bg-brand',
                )}
            />
            <Icon
                size={13}
                className={cn(
                    'shrink-0',
                    isFailed
                        ? 'text-danger/80'
                        : active
                          ? 'text-brand'
                          : 'text-text-tertiary group-hover:text-text-secondary',
                )}
            />
            <span
                className="min-w-0 max-w-[160px] truncate text-[12.5px] font-medium leading-none"
                title={host.display_name}
            >
                {host.display_name}
            </span>
            <span className="shrink-0 font-mono text-[10px] uppercase leading-none text-text-tertiary">
                {host.os}
            </span>
            {isFailed ? (
                <span className="shrink-0 rounded-xs bg-danger/15 px-1.5 py-0.5 text-[10px] font-medium leading-none text-danger">
                    中断
                </span>
            ) : (
                <span
                    className={cn(
                        'shrink-0 rounded-xs px-1.5 py-0.5 font-mono text-[10px] leading-none tabular-nums',
                        active
                            ? 'bg-brand/15 text-brand'
                            : 'bg-inset/80 text-text-tertiary group-hover:text-text-secondary',
                    )}
                >
                    {installed}/{total}
                </span>
            )}
        </button>
    );
});
HostTab.displayName = 'HostTab';

export default HostSwitcher;

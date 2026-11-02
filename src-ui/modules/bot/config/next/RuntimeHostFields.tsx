import { Box, Terminal } from 'lucide-react';
import { Select } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type { RuntimeSelection } from '../../../../core/domain/bot/runtime-selection';
import type { DeploymentType } from '../../../../core/ipc/generated/domain/DeploymentType';

const DEPLOYMENTS = [
    { value: 'native', label: '直接运行', icon: Terminal },
    { value: 'docker', label: '容器', icon: Box },
] as const;

export function RuntimeHostFields({
    selection,
    serverItems,
    onHostChange,
    onDeploymentChange,
}: {
    selection: RuntimeSelection;
    serverItems: { value: string; label: string }[];
    onHostChange: (value: string) => void;
    onDeploymentChange: (value: DeploymentType) => void;
}) {
    const showServers = selection.isRemote && serverItems.length > 0;
    return (
        <div className={cn('grid grid-cols-1 gap-3', showServers && 'sm:grid-cols-2')}>
            {showServers && (
                <Select
                    label="目标远程主机"
                    items={serverItems}
                    value={selection.selectedServerId}
                    onValueChange={onHostChange}
                    placeholder="选择主机"
                />
            )}
            <div className="flex flex-col gap-1.5">
                <span className="text-xs font-medium text-text-secondary">启动方式</span>
                <div
                    role="radiogroup"
                    aria-label="启动方式"
                    className="grid h-[38px] grid-cols-2 gap-1 rounded-sm border border-border-subtle bg-inset p-0.5"
                >
                    {DEPLOYMENTS.map(({ value, label, icon: Icon }) => {
                        const selected = selection.deploymentType === value;
                        return (
                            <button
                                key={value}
                                type="button"
                                role="radio"
                                aria-checked={selected}
                                disabled={value === 'docker' && !selection.dockerAllowed}
                                onClick={() => onDeploymentChange(value)}
                                className={cn(
                                    'inline-flex items-center justify-center gap-1.5 rounded-xs px-2.5 text-xs font-medium transition-all',
                                    'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                                    selected
                                        ? 'border border-border/50 bg-surface text-text shadow-xs'
                                        : 'border border-transparent text-text-tertiary hover:text-text',
                                )}
                            >
                                <Icon
                                    size={13}
                                    className={selected ? 'text-brand' : 'text-text-tertiary'}
                                />
                                <span>{label}</span>
                            </button>
                        );
                    })}
                </div>
            </div>
        </div>
    );
}

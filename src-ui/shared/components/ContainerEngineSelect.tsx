import * as RadioGroup from '@radix-ui/react-radio-group';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';
import { cn } from '../utils/cn';

const ENGINES = [
    { value: 'docker', label: 'Docker' },
    { value: 'podman', label: 'Podman' },
] as const;

export function ContainerEngineSelect({
    value,
    disabled,
    onValueChange,
    size = 'md',
}: {
    value?: ContainerEngine;
    disabled: boolean;
    onValueChange: (engine: ContainerEngine) => void;
    size?: 'sm' | 'md';
}) {
    return (
        <RadioGroup.Root
            aria-label="选择容器引擎"
            orientation="horizontal"
            value={value ?? ''}
            disabled={disabled}
            onValueChange={(engine) => onValueChange(engine as ContainerEngine)}
            className={cn(
                'inline-flex shrink-0 items-center gap-0.5 rounded-md border border-border-subtle bg-canvas p-0.5',
                size === 'sm' && 'h-7',
            )}
        >
            {ENGINES.map(({ value: engine, label }) => (
                <RadioGroup.Item
                    key={engine}
                    value={engine}
                    className={cn(
                        'inline-flex items-center justify-center rounded-sm px-2.5 text-xs font-medium text-text-secondary transition-colors hover:bg-field/60 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-60 data-[state=checked]:bg-surface data-[state=checked]:text-text data-[state=checked]:shadow-sm data-[state=checked]:ring-1 data-[state=checked]:ring-border-subtle',
                        size === 'sm' ? 'h-full min-w-[4.5rem]' : 'h-8 min-w-[5.5rem]',
                    )}
                >
                    {label}
                </RadioGroup.Item>
            ))}
        </RadioGroup.Root>
    );
}

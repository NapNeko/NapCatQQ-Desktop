import * as RadioGroup from '@radix-ui/react-radio-group';
import { Check } from 'lucide-react';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';

const ENGINES = [
    { value: 'docker', label: 'Docker' },
    { value: 'podman', label: 'Podman' },
] as const;

export function ContainerEngineSelect({
    value,
    disabled,
    onValueChange,
}: {
    value: ContainerEngine;
    disabled: boolean;
    onValueChange: (engine: ContainerEngine) => void;
}) {
    return (
        <RadioGroup.Root
            aria-label="选择容器引擎"
            orientation="horizontal"
            value={value}
            disabled={disabled}
            onValueChange={(engine) => onValueChange(engine as ContainerEngine)}
            className="inline-flex shrink-0 items-center gap-0.5 rounded-md border border-border-subtle bg-canvas p-0.5"
        >
            {ENGINES.map(({ value: engine, label }) => (
                <RadioGroup.Item
                    key={engine}
                    value={engine}
                    className="inline-flex h-8 min-w-[5.5rem] items-center justify-center gap-1.5 rounded-sm px-2.5 text-xs font-medium text-text-secondary transition-colors hover:bg-field/60 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-60 data-[state=checked]:bg-surface data-[state=checked]:text-text data-[state=checked]:shadow-sm data-[state=checked]:ring-1 data-[state=checked]:ring-border-subtle"
                >
                    <Check
                        size={12}
                        aria-hidden
                        className={value === engine ? 'text-brand' : 'invisible'}
                    />
                    {label}
                </RadioGroup.Item>
            ))}
        </RadioGroup.Root>
    );
}

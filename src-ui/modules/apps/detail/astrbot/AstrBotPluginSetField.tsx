import { Select, StringListField } from '../../../../shared/ui';

export function AstrBotPluginSetField({
    label,
    hint,
    value,
    disabled,
    onChange,
}: {
    label: string;
    hint?: string;
    value: unknown;
    disabled?: boolean;
    onChange: (value: string[]) => void;
}) {
    const valid = Array.isArray(value) && value.every((item) => typeof item === 'string');
    const plugins: string[] = valid ? value : [];
    const mode = valid ? (plugins.includes('*') ? 'all' : 'selected') : 'invalid';

    return (
        <div className="flex flex-col gap-3">
            <Select
                label={label}
                hint={hint}
                value={mode}
                items={[
                    ...(valid ? [] : [{ value: 'invalid', label: '当前值无效', disabled: true }]),
                    { value: 'all', label: '全部已启用插件' },
                    { value: 'selected', label: '指定插件' },
                ]}
                disabled={disabled}
                onValueChange={(next) => {
                    if (next === 'all') onChange(['*']);
                    else if (next === 'selected') onChange([]);
                }}
            />
            {mode === 'selected' && (
                <StringListField
                    label="插件名称"
                    hint="留空时不启用任何插件"
                    placeholder="插件名称，回车添加"
                    value={plugins}
                    disabled={disabled}
                    onChange={onChange}
                />
            )}
        </div>
    );
}

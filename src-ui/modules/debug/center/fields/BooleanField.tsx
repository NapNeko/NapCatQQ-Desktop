// 布尔参数：开关，外加「不填」。没填和填了 false 在上游不一定一样（没填走上游默认），所以要分得出来。
// NapCat 的布尔参数也收字符串 "true" / "false"，显示时一并认。

import { X } from 'lucide-react';
import { Switch } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { IconTip } from '../centerParts';
import type { FieldProps } from './fieldKit';

function asBool(v: unknown): boolean | null {
    if (v === true || v === 'true') return true;
    if (v === false || v === 'false') return false;
    return null;
}

export function BooleanField({
    field,
    value,
    onChange,
    invalid,
    inputId,
    describedBy,
    disabled,
}: FieldProps) {
    const current = asBool(value);
    const unset = value === undefined;
    const fallback = asBool(field.defaultValue);
    let state: string;
    if (unset) state = fallback === null ? '没填' : `没填（默认 ${fallback}）`;
    else if (current === null) state = `其它值：${JSON.stringify(value)}`;
    else state = String(current);

    return (
        <div className="flex h-[38px] items-center gap-2.5">
            <Switch
                id={inputId}
                checked={current ?? (unset ? fallback === true : false)}
                onCheckedChange={(on) => onChange(on)}
                disabled={disabled}
                aria-invalid={invalid || undefined}
                aria-describedby={describedBy}
                className={cn(
                    unset && !invalid && 'opacity-60',
                    invalid &&
                        '[&>button]:ring-1 [&>button]:ring-danger [&>button]:focus-visible:ring-danger',
                )}
            />
            <span
                className={
                    unset || current === null
                        ? 'text-xs text-text-tertiary'
                        : 'font-mono text-[13px] text-text'
                }
            >
                {state}
            </span>
            {!unset && !field.required && (
                <IconTip
                    icon={X}
                    label="改回不填"
                    size="sm"
                    onClick={() => onChange(undefined)}
                    disabled={disabled}
                />
            )}
        </div>
    );
}

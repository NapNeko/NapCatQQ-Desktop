// 枚举参数：下拉。选项的值可能是字符串 / 数字 / 布尔，下拉只认字符串，所以按下标当键。
// JSON 里写了枚举之外的值时，下拉显示成「其它值」而不是悄悄换成第一项。

import { Select, type SelectItem } from '../../../../shared/ui';
import { sameJson } from '../viewHelpers';
import type { FieldProps } from './fieldKit';

const UNSET = '__unset__';
const OTHER = '__other__';

// 保留 Select 的无效状态，错误文字由表单行统一显示。
export function EnumField({ field, value, onChange, invalid, inputId, disabled }: FieldProps) {
    const options = field.enumValues ?? [];
    const index = options.findIndex((o) => sameJson(o.value, value));
    const items: Array<SelectItem<string>> = [];
    if (!field.required) {
        const fallback =
            field.defaultValue !== undefined ? `（默认 ${String(field.defaultValue)}）` : '';
        items.push({
            value: UNSET,
            label: <span className="text-text-tertiary">不填{fallback}</span>,
        });
    }
    options.forEach((o, i) => {
        items.push({
            value: String(i),
            label: o.label === String(o.value) ? o.label : `${o.label}（${String(o.value)}）`,
        });
    });
    const other = value !== undefined && index < 0;
    if (other)
        items.push({ value: OTHER, label: `其它值：${JSON.stringify(value)}`, disabled: true });

    const selected = other
        ? OTHER
        : index >= 0
          ? String(index)
          : field.required
            ? undefined
            : UNSET;

    return (
        <Select
            id={inputId}
            items={items}
            value={selected}
            placeholder="选一个"
            disabled={disabled}
            error={invalid ? '参数无效' : undefined}
            className="min-w-0 [&>p]:sr-only"
            onValueChange={(v) => {
                if (v === UNSET) onChange(undefined);
                else if (v !== OTHER) onChange(options[Number(v)]?.value);
            }}
        />
    );
}

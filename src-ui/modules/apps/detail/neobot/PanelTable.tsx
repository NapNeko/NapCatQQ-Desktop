// 可选择的数据表保留键盘入口，行内按钮独立触发。
import type { ReactNode } from 'react';
import { text, type PanelObject } from '../../../../core/domain/apps/neobotWorkspace';
import { Button } from '../../../../shared/ui';

export interface PanelColumn {
    key: string;
    label: string;
    align?: 'left' | 'right';
    mono?: boolean;
    render?: (row: PanelObject, index: number) => ReactNode;
    width?: string;
}

function columnText(value: unknown): ReactNode {
    if (value === null || value === undefined) return '—';
    if (typeof value === 'boolean') return value ? '是' : '否';
    if (Array.isArray(value)) return value.map(text).filter(Boolean).join('、') || '—';
    if (typeof value === 'object') return '—';
    return text(value) || '—';
}

export function PanelTable({
    rows,
    columns,
    empty = '暂无记录',
    rowKey,
    onRowClick,
    selectedKey,
}: {
    rows: PanelObject[];
    columns: PanelColumn[];
    empty?: ReactNode;
    rowKey?: (row: PanelObject, index: number) => string;
    onRowClick?: (row: PanelObject) => void;
    selectedKey?: string;
}) {
    if (!rows.length) return <p className="text-xs text-text-tertiary">{empty}</p>;
    const keyOf = (row: PanelObject, index: number) =>
        rowKey?.(row, index) ?? (text(row.id) || text(row.key) || text(row.name) || String(index));
    return (
        <div className="-mx-4 overflow-x-auto px-4">
            <table className="w-full min-w-[32rem] border-collapse text-left">
                <thead>
                    <tr>
                        {columns.map((col) => (
                            <th
                                key={col.key}
                                scope="col"
                                style={col.width ? { width: col.width } : undefined}
                                className={
                                    'border-b border-border-subtle pb-1.5 pr-3 text-2xs font-medium text-text-tertiary last:pr-0 ' +
                                    (col.align === 'right' ? 'text-right' : 'text-left')
                                }
                            >
                                {col.label}
                            </th>
                        ))}
                    </tr>
                </thead>
                <tbody>
                    {rows.map((row, index) => {
                        const key = keyOf(row, index);
                        return (
                            <tr
                                key={key}
                                onClick={
                                    onRowClick
                                        ? (event) => {
                                              if (
                                                  event.target instanceof Element &&
                                                  event.target.closest(
                                                      'button, a, input, select, textarea, [role="button"]',
                                                  )
                                              )
                                                  return;
                                              onRowClick(row);
                                          }
                                        : undefined
                                }
                                className={
                                    'border-b border-border-subtle/60 last:border-0 ' +
                                    (onRowClick ? 'cursor-pointer hover:bg-inset/60 ' : '') +
                                    (selectedKey === key ? 'bg-brand-soft/40' : '')
                                }
                            >
                                {columns.map((col, columnIndex) => (
                                    <td
                                        key={col.key}
                                        className={
                                            'max-w-[26rem] break-words py-2 pr-3 align-middle text-xs text-text-secondary last:pr-0 ' +
                                            (col.align === 'right'
                                                ? 'text-right tabular-nums '
                                                : 'text-left ') +
                                            (col.mono ? 'font-mono-strict' : '')
                                        }
                                    >
                                        {col.render ? (
                                            col.render(row, index)
                                        ) : onRowClick && columnIndex === 0 ? (
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                aria-pressed={selectedKey === key}
                                                onClick={() => onRowClick(row)}
                                            >
                                                {columnText(row[col.key])}
                                            </Button>
                                        ) : (
                                            columnText(row[col.key])
                                        )}
                                    </td>
                                ))}
                            </tr>
                        );
                    })}
                </tbody>
            </table>
        </div>
    );
}

export function RecordTable({
    items,
    columns,
    onSelect,
}: {
    items: PanelObject[];
    columns: readonly (readonly [string, string])[];
    onSelect?: (item: PanelObject) => void;
}) {
    return (
        <PanelTable
            rows={items}
            columns={columns.map(([key, label]) => ({ key, label }))}
            onRowClick={onSelect}
        />
    );
}

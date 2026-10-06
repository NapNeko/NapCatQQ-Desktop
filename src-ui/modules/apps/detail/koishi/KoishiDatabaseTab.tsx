// 数据库：浏览实例库里的表（dataview 插件那套：`database` 推送拿表目录，`database/get` 翻行）。
// 第一版只读：看结构、翻页看行；改数据去插件页或控制台。

import { useMemo, useState } from 'react';
import { ChevronLeft, ChevronRight, Database, RotateCw, Table2 } from 'lucide-react';
import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    useKoishiDatabaseRows,
    useKoishiDatabaseTables,
} from '../../../../hooks/apps/useKoishiConsole';
import { PaneLoading } from '../PaneStatus';
import type { AppInstance } from '../../../../core/ipc/types';

const PAGE_SIZE = 50;

export const KoishiDatabaseTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const running = instance.state === 'running';
    const tables = useKoishiDatabaseTables(instance.id, running);
    const [table, setTable] = useState<string | null>(null);
    const [page, setPage] = useState(0);
    const rows = useKoishiDatabaseRows(instance.id, table, page * PAGE_SIZE, PAGE_SIZE);

    const current = (tables.data ?? []).find((t) => t.name === table);
    const columns = useMemo(() => {
        const fromFields = Object.keys((current?.fields ?? {}) as Record<string, unknown>);
        if (fromFields.length > 0) return fromFields;
        const first = rows.data?.[0];
        return first ? Object.keys(first) : [];
    }, [current, rows.data]);

    const pick = (name: string) => {
        setTable(name);
        setPage(0);
    };

    return (
        <div className="flex min-h-0 flex-1 gap-4 pb-3">
            <aside className="flex w-[220px] shrink-0 flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface shadow-card">
                <div className="flex items-center justify-between border-b border-border-subtle/70 px-3 py-2.5">
                    <span className="text-2xs text-text-tertiary">
                        {tables.data?.length ?? 0} 张表
                    </span>
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={!running || tables.isFetching}
                        title="刷新表目录"
                        onClick={() => void tables.refetch()}
                    >
                        <RotateCw size={13} className={cn(tables.isFetching && 'animate-spin')} />
                    </Button>
                </div>
                <ul className="min-h-0 flex-1 overflow-y-auto p-1.5">
                    {!running ? (
                        <li className="px-3 py-6 text-center text-xs text-text-tertiary">
                            启动实例后可读
                        </li>
                    ) : (
                        (tables.data ?? []).map((t) => (
                            <li key={t.name}>
                                <button
                                    type="button"
                                    onClick={() => pick(t.name)}
                                    className={cn(
                                        'flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] transition-colors',
                                        table === t.name
                                            ? 'bg-brand-soft text-text'
                                            : 'text-text-secondary hover:bg-inset',
                                    )}
                                >
                                    <Table2 size={13} className="shrink-0 text-text-tertiary" />
                                    <span className="min-w-0 flex-1 truncate font-mono">
                                        {t.name}
                                    </span>
                                    {t.count !== null && (
                                        <span className="shrink-0 text-2xs tabular-nums text-text-disabled">
                                            {t.count}
                                        </span>
                                    )}
                                </button>
                            </li>
                        ))
                    )}
                </ul>
            </aside>

            <section className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface shadow-card">
                {!table ? (
                    <Empty text="左边挑一张表；行数据是活的，改翻页才重新取" />
                ) : rows.isLoading ? (
                    <PaneLoading text={`正在读取 ${table}…`} />
                ) : rows.error ? (
                    <Empty text={rows.error.message} error />
                ) : (
                    <>
                        <div className="flex shrink-0 items-center gap-2 border-b border-border-subtle/70 px-3.5 py-2">
                            <span className="font-mono text-[13px] font-medium text-text">
                                {table}
                            </span>
                            {current && current.primary.length > 0 && (
                                <span className="text-2xs text-text-tertiary">
                                    主键 {current.primary.join(' + ')}
                                </span>
                            )}
                            <div className="ml-auto flex items-center gap-1">
                                <Button
                                    size="sm"
                                    variant="ghost"
                                    disabled={page === 0 || rows.isFetching}
                                    onClick={() => setPage((p) => p - 1)}
                                >
                                    <ChevronLeft size={13} />
                                    上一页
                                </Button>
                                <span className="px-1 text-2xs tabular-nums text-text-tertiary">
                                    {page + 1}
                                </span>
                                <Button
                                    size="sm"
                                    variant="ghost"
                                    disabled={
                                        rows.isFetching || (rows.data?.length ?? 0) < PAGE_SIZE
                                    }
                                    onClick={() => setPage((p) => p + 1)}
                                >
                                    下一页
                                    <ChevronRight size={13} />
                                </Button>
                            </div>
                        </div>
                        <div className="min-h-0 flex-1 overflow-auto">
                            {(rows.data ?? []).length === 0 ? (
                                <Empty text="这张表是空的" />
                            ) : (
                                <table className="w-full border-collapse text-[12.5px]">
                                    <thead className="sticky top-0 z-10 bg-surface">
                                        <tr>
                                            {columns.map((c) => (
                                                <th
                                                    key={c}
                                                    className="border-b border-border-subtle px-3 py-2 text-left font-mono text-2xs font-medium text-text-tertiary"
                                                >
                                                    {c}
                                                </th>
                                            ))}
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {(rows.data ?? []).map((r, i) => (
                                            <tr
                                                key={i}
                                                className="transition-colors hover:bg-inset/50"
                                            >
                                                {columns.map((c) => (
                                                    <td
                                                        key={c}
                                                        className="max-w-[260px] truncate border-b border-border-subtle/50 px-3 py-1.5 text-text-secondary"
                                                        title={cellText(r[c])}
                                                    >
                                                        {cellText(r[c])}
                                                    </td>
                                                ))}
                                            </tr>
                                        ))}
                                    </tbody>
                                </table>
                            )}
                        </div>
                    </>
                )}
            </section>
        </div>
    );
};

function cellText(v: unknown): string {
    if (v === null || v === undefined) return '';
    if (typeof v === 'string') return v;
    if (typeof v === 'number' || typeof v === 'boolean') return String(v);
    return JSON.stringify(v);
}

function Empty({ text, error }: { text: string; error?: boolean }) {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
            {!error && (
                <span className="flex h-11 w-11 items-center justify-center rounded-lg bg-inset text-text-tertiary">
                    <Database size={20} />
                </span>
            )}
            <p
                className={cn(
                    'max-w-md text-xs leading-relaxed',
                    error ? 'text-danger' : 'text-text-tertiary',
                )}
            >
                {text}
            </p>
        </div>
    );
}

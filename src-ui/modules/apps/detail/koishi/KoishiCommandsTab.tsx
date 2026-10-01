// 指令：实例里所有指令的别名、权限、冷却、次数。来自控制台 entry 推送（指令管理器那份），
// 改别名走上游 command/aliases，改配置走 command/update——和网页控制台同一条协议。

import { useMemo, useState } from 'react';
import { ChevronRight, CornerDownRight, TerminalSquare } from 'lucide-react';
import { Badge, Button, NumberField, StringListField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { useKoishiCommandOps, useKoishiCommands } from '../../../../hooks/apps/useKoishiConsole';
import { PaneLoading } from '../PaneStatus';
import type { AppInstance, KoishiCommandRow } from '../../../../core/ipc/types';

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

export const KoishiCommandsTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const running = instance.state === 'running';
    const commands = useKoishiCommands(instance.id, running);
    const [openName, setOpenName] = useState<string | null>(null);
    const [query, setQuery] = useState('');

    const rows = useMemo(() => {
        const list = commands.data ?? [];
        const q = query.trim().toLowerCase();
        if (!q) return list;
        return list.filter((c) => c.name.toLowerCase().includes(q) || c.aliases.some((a) => a.toLowerCase().includes(q)));
    }, [commands.data, query]);

    const childOf = useMemo(() => {
        const map = new Map<string, string>();
        for (const c of commands.data ?? []) for (const ch of c.children) map.set(ch, c.name);
        return map;
    }, [commands.data]);

    return (
        <div className="flex min-w-0 flex-col gap-3 pb-8">
            <div className="flex flex-wrap items-center gap-2">
                <input
                    type="search"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="搜指令或别名"
                    aria-label="搜指令或别名"
                    className="h-8 w-56 rounded-md border border-border-subtle bg-inset/60 px-2.5 text-[12.5px] text-text outline-none transition-colors placeholder:text-text-tertiary focus:border-brand/50"
                />
                <span className="text-2xs text-text-tertiary">
                    改动立刻写进 koishi.yml 的 commands 插件配置并重载，和在网页控制台里改是同一回事
                </span>
            </div>

            {!running ? (
                <Empty text="启动实例后这里列出所有指令" />
            ) : commands.isLoading ? (
                <PaneLoading text="正在读取指令列表…" />
            ) : commands.error ? (
                <Empty text={commands.error.message} error />
            ) : rows.length === 0 ? (
                <Empty text={query ? `没有匹配「${query.trim()}」的指令` : '还没有指令'} />
            ) : (
                <ul className="flex flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface">
                    {rows.map((c) => (
                        <Row
                            key={c.name}
                            row={c}
                            parent={childOf.get(c.name)}
                            open={openName === c.name}
                            onToggle={() => setOpenName((n) => (n === c.name ? null : c.name))}
                            instance={instance}
                        />
                    ))}
                </ul>
            )}
        </div>
    );
};

function Empty({ text, error }: { text: string; error?: boolean }) {
    return (
        <div className="flex flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-border-subtle px-8 py-14 text-center">
            <span className="flex h-11 w-11 items-center justify-center rounded-lg bg-inset text-text-tertiary">
                <TerminalSquare size={20} />
            </span>
            <p className={cn('max-w-md text-xs leading-relaxed', error ? 'text-danger' : 'text-text-tertiary')}>{text}</p>
        </div>
    );
}

function Row({
    row,
    parent,
    open,
    onToggle,
    instance,
}: {
    row: KoishiCommandRow;
    parent?: string;
    open: boolean;
    onToggle: () => void;
    instance: AppInstance;
}) {
    const ops = useKoishiCommandOps(instance.id);
    const [aliases, setAliases] = useState<string[] | null>(null);
    const [authority, setAuthority] = useState<number | null>(null);
    const [minInterval, setMinInterval] = useState<number | null>(null);
    const [maxUsage, setMaxUsage] = useState<number | null>(null);

    // 展开时把草稿初始化成当前生效值；合上丢掉
    const curAliases = aliases ?? row.aliases;
    const curAuthority = authority ?? num(row.config.authority);
    const curInterval = minInterval ?? num(row.config.minInterval);
    const curUsage = maxUsage ?? num(row.config.maxUsage);
    const dirty =
        aliases !== null || authority !== null || minInterval !== null || maxUsage !== null;
    const saving = ops.update.isPending || ops.aliases.isPending;

    const save = async () => {
        if (aliases !== null) {
            await ops.aliases.mutateAsync({ name: row.name, aliases });
        }
        const config: Record<string, unknown> = {};
        if (authority !== null) config.authority = authority;
        if (minInterval !== null) config.minInterval = minInterval;
        if (maxUsage !== null) config.maxUsage = maxUsage;
        if (Object.keys(config).length > 0) {
            await ops.update.mutateAsync({ name: row.name, config });
        }
        setAliases(null);
        setAuthority(null);
        setMinInterval(null);
        setMaxUsage(null);
    };

    return (
        <li className="border-b border-border-subtle/70 last:border-b-0">
            <button
                type="button"
                aria-expanded={open}
                onClick={onToggle}
                className="flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-inset/60"
            >
                <ChevronRight
                    size={14}
                    className={cn('shrink-0 text-text-tertiary transition-transform duration-200', open && 'rotate-90')}
                />
                <span className="flex min-w-0 flex-1 items-center gap-2">
                    <span className="font-mono text-[13px] font-medium text-text">
                        {parent && <CornerDownRight size={11} className="mr-1 inline text-text-disabled" />}
                        {row.name}
                    </span>
                    {row.aliases.length > 0 && (
                        <span className="truncate text-2xs text-text-tertiary">
                            {row.aliases.map((a) => `「${a}」`).join(' ')}
                        </span>
                    )}
                </span>
                <span className="flex shrink-0 items-center gap-1.5">
                    {num(row.config.authority) !== null && num(row.config.authority) !== 1 && (
                        <Badge tone="warning">权限 {String(num(row.config.authority))}</Badge>
                    )}
                    {row.created && <Badge tone="brand">自建</Badge>}
                    <span className="text-2xs text-text-disabled">
                        {row.paths.length > 0 ? `插件 ${row.paths.join('、')}` : '内置'}
                    </span>
                </span>
            </button>
            {open && (
                <div className="border-t border-border-subtle/60 bg-inset/40 px-4 py-4">
                    <div className="grid max-w-3xl gap-x-5 gap-y-4 sm:grid-cols-3">
                        <NumberField
                            label="权限等级"
                            hint="authority；1 = 所有人"
                            value={curAuthority}
                            min={0}
                            disabled={saving}
                            onValueChange={(v) => setAuthority(v ?? 0)}
                        />
                        <NumberField
                            label="冷却（毫秒）"
                            hint="minInterval；同一个人短时间内不重复触发"
                            value={curInterval}
                            min={0}
                            disabled={saving}
                            onValueChange={(v) => setMinInterval(v ?? 0)}
                        />
                        <NumberField
                            label="次数上限"
                            hint="maxUsage；每个周期能用几次"
                            value={curUsage}
                            min={0}
                            disabled={saving}
                            onValueChange={(v) => setMaxUsage(v ?? 0)}
                        />
                    </div>
                    <div className="mt-4 max-w-3xl">
                        <StringListField
                            label="别名"
                            hint="输入后回车加一个；原名不用重复写"
                            value={curAliases}
                            disabled={saving}
                            onChange={setAliases}
                        />
                    </div>
                    <div className="mt-4 flex items-center gap-2">
                        <Button size="sm" variant="primary" disabled={!dirty || saving} onClick={() => void save()}>
                            {saving ? '保存中…' : '保存'}
                        </Button>
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={!dirty || saving}
                            onClick={() => {
                                setAliases(null);
                                setAuthority(null);
                                setMinInterval(null);
                                setMaxUsage(null);
                            }}
                        >
                            还原
                        </Button>
                    </div>
                </div>
            )}
        </li>
    );
}

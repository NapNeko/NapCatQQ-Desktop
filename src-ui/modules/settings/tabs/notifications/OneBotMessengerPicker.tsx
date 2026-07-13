// OneBot 发送方 Bot 选择器（按主机分组）

import { useState } from 'react';
import { X } from 'lucide-react';
import { settingsService } from '../../../../core/services/settings.service';
import { Badge, Button, TextField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';

export type OneBotCandidate = Awaited<
    ReturnType<typeof settingsService.listOneBotMessengerCandidates>
>[number];

function backendLabel(backend: string): string {
    return backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
}

function stateLabel(state: string): string {
    switch (state) {
        case 'running':
            return '运行中';
        case 'starting':
            return '启动中';
        case 'stopping':
            return '停止中';
        case 'crashed':
            return '异常退出';
        case 'repairing':
            return '修复中';
        default:
            return '已停止';
    }
}

export function OneBotMessengerPicker({
    selected,
    candidates,
    loading,
    enablingId,
    onChange,
    onEnsureHttp,
}: {
    selected: string[];
    candidates: OneBotCandidate[];
    loading: boolean;
    enablingId: string | null;
    onChange: (next: string[]) => void;
    onEnsureHttp: (botId: string) => void;
}) {
    const [query, setQuery] = useState('');
    const selectedSet = new Set(selected);
    const selectedMissing = selected.filter(
        (id) => !candidates.some((item) => item.bot_id === id),
    );
    const localEligible = candidates.filter(
        (item) => item.scope !== 'remote' && item.eligible,
    ).length;
    const remoteWatchReady = candidates.filter(
        (item) => item.scope === 'remote' && item.has_local_http,
    ).length;

    const sortCandidates = (list: OneBotCandidate[]) =>
        [...list].sort((a, b) => {
            const aSelected = selectedSet.has(a.bot_id) ? 1 : 0;
            const bSelected = selectedSet.has(b.bot_id) ? 1 : 0;
            if (aSelected !== bSelected) return bSelected - aSelected;
            if (a.eligible !== b.eligible) return Number(b.eligible) - Number(a.eligible);
            if (a.has_local_http !== b.has_local_http) {
                return Number(b.has_local_http) - Number(a.has_local_http);
            }
            const aRunning = a.state === 'running' ? 1 : 0;
            const bRunning = b.state === 'running' ? 1 : 0;
            if (aRunning !== bRunning) return bRunning - aRunning;
            return (a.name || a.bot_id).localeCompare(b.name || b.bot_id, 'zh-CN');
        });

    const filtered = (() => {
        const q = query.trim().toLowerCase();
        if (!q) return candidates;
        return candidates.filter((item) => {
            const hay =
                `${item.name} ${item.bot_id} ${item.backend_type} ${item.state} ${item.server_label ?? ''} ${item.server_id ?? ''}`.toLowerCase();
            return hay.includes(q);
        });
    })();

    type HostGroup = {
        key: string;
        label: string;
        isLocal: boolean;
        items: OneBotCandidate[];
    };

    const groups: HostGroup[] = (() => {
        const map = new Map<string, HostGroup>();
        for (const item of filtered) {
            const isLocal = item.scope !== 'remote';
            const key = isLocal
                ? 'local'
                : `remote:${item.server_id ?? item.server_label ?? 'unknown'}`;
            const label = isLocal
                ? '本机'
                : item.server_label?.trim() || item.server_id || '远端';
            let group = map.get(key);
            if (!group) {
                group = { key, label, isLocal, items: [] };
                map.set(key, group);
            }
            group.items.push(item);
        }
        const list = [...map.values()].map((g) => ({
            ...g,
            items: sortCandidates(g.items),
        }));
        list.sort((a, b) => {
            if (a.isLocal !== b.isLocal) return a.isLocal ? -1 : 1;
            return a.label.localeCompare(b.label, 'zh-CN');
        });
        return list;
    })();

    const toggle = (botId: string) => {
        if (selectedSet.has(botId)) {
            onChange(selected.filter((id) => id !== botId));
            return;
        }
        onChange([...selected, botId]);
    };

    const moveSelected = (botId: string, direction: -1 | 1) => {
        const index = selected.indexOf(botId);
        if (index < 0) return;
        const nextIndex = index + direction;
        if (nextIndex < 0 || nextIndex >= selected.length) return;
        const next = [...selected];
        const [item] = next.splice(index, 1);
        next.splice(nextIndex, 0, item);
        onChange(next);
    };

    const statusBadge = (candidate: OneBotCandidate) => {
        const enabling = enablingId === candidate.bot_id;
        const isRemote = candidate.scope === 'remote';
        if (candidate.eligible) {
            return (
                <Badge tone="success" appearance="soft">
                    Desktop 可发
                </Badge>
            );
        }
        if (candidate.has_local_http) {
            return (
                <Badge tone="success" appearance="soft">
                    {isRemote ? '同机可发' : 'HTTP 就绪'}
                </Badge>
            );
        }
        if (candidate.can_enable_http) {
            return (
                <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    disabled={enabling}
                    onClick={() => onEnsureHttp(candidate.bot_id)}
                >
                    {enabling ? '配置中…' : '启用 HTTP'}
                </Button>
            );
        }
        return (
            <Badge tone="warning" appearance="soft">
                暂不可用
            </Badge>
        );
    };

    const chipTone = (candidate: OneBotCandidate | undefined) => {
        if (!candidate) return false;
        if (candidate.eligible) return true;
        if (candidate.has_local_http) return true;
        return false;
    };

    return (
        <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-sm border border-border-subtle bg-field">
            <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-3 py-3">
                <div className="flex items-center justify-between gap-2">
                    <div className="min-w-0">
                        <div className="flex items-center gap-2">
                            <p className="text-[13px] font-medium text-text">
                                发送方 Bot
                            </p>
                            <Badge tone="neutral" appearance="soft">
                                已选 {selected.length}
                            </Badge>
                        </div>
                        <p className="mt-0.5 text-[11.5px] leading-relaxed text-text-tertiary">
                            按主机分组；同机多 Bot 冗余，不会跨服务器发 OneBot
                            {localEligible > 0 ? ` · 本机可发 ${localEligible}` : ''}
                            {remoteWatchReady > 0
                                ? ` · 远端同机 ${remoteWatchReady}`
                                : ''}
                        </p>
                    </div>
                </div>

                {selected.length > 0 ? (
                    <div className="flex flex-wrap gap-1.5">
                        {selected.map((id, index) => {
                            const candidate = candidates.find(
                                (item) => item.bot_id === id,
                            );
                            const label = candidate?.name || id;
                            const isRemote = candidate?.scope === 'remote';
                            return (
                                <span
                                    key={id}
                                    className={cn(
                                        'inline-flex max-w-full items-center gap-1 rounded-sm border px-1.5 py-1 text-[11.5px]',
                                        chipTone(candidate)
                                            ? 'border-success/30 bg-success-soft text-text'
                                            : 'border-warning/30 bg-warning-soft text-text',
                                    )}
                                >
                                    <span className="shrink-0 rounded-xs bg-field/70 px-1 py-px font-mono text-[10px] text-text-tertiary">
                                        {index + 1}
                                    </span>
                                    {isRemote ? (
                                        <span className="shrink-0 rounded-xs bg-field/70 px-1 py-px text-[10px] text-text-tertiary">
                                            远端
                                        </span>
                                    ) : null}
                                    <span className="truncate">{label}</span>
                                    {selected.length > 1 ? (
                                        <span className="flex shrink-0 items-center">
                                            <button
                                                type="button"
                                                aria-label={`上移 ${label}`}
                                                disabled={index === 0}
                                                onClick={() => moveSelected(id, -1)}
                                                className="rounded-xs px-0.5 text-text-tertiary transition-colors hover:text-text disabled:opacity-30"
                                            >
                                                ↑
                                            </button>
                                            <button
                                                type="button"
                                                aria-label={`下移 ${label}`}
                                                disabled={index === selected.length - 1}
                                                onClick={() => moveSelected(id, 1)}
                                                className="rounded-xs px-0.5 text-text-tertiary transition-colors hover:text-text disabled:opacity-30"
                                            >
                                                ↓
                                            </button>
                                        </span>
                                    ) : null}
                                    <button
                                        type="button"
                                        aria-label={`移除 ${label}`}
                                        onClick={() =>
                                            onChange(
                                                selected.filter((item) => item !== id),
                                            )
                                        }
                                        className="rounded-xs p-0.5 text-text-tertiary transition-colors hover:bg-field hover:text-text"
                                    >
                                        <X size={11} />
                                    </button>
                                </span>
                            );
                        })}
                    </div>
                ) : (
                    <div className="rounded-sm border border-dashed border-border-subtle bg-inset/25 px-2.5 py-2 text-[11.5px] text-text-tertiary">
                        从下方按主机勾选发送方；本机供 Desktop 投递，远端供该机 ncd-watch
                    </div>
                )}

                <TextField
                    name="onebot-messenger-search"
                    value={query}
                    placeholder="搜索名称 / QQ / 后端 / 主机"
                    onValueChange={setQuery}
                />
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto">
                {loading ? (
                    <p className="px-3 py-6 text-center text-[12px] text-text-tertiary">
                        正在加载发送方候选…
                    </p>
                ) : groups.length === 0 && selectedMissing.length === 0 ? (
                    <div className="flex h-full min-h-[10rem] flex-col items-center justify-center gap-1 px-4 py-8 text-center">
                        <p className="text-[13px] text-text-secondary">
                            {candidates.length === 0
                                ? '还没有可作发送方的 Bot'
                                : '没有匹配的 Bot'}
                        </p>
                        <p className="text-[11.5px] text-text-tertiary">
                            {candidates.length === 0
                                ? '先在 Bot 列表添加本机或远端实例，再回来配置'
                                : '试试换个关键词'}
                        </p>
                    </div>
                ) : (
                    <div className="pb-1">
                        {groups.map((group) => (
                            <div key={group.key}>
                                <div className="sticky top-0 z-[1] flex items-center gap-2 border-b border-border-subtle/80 bg-inset/90 px-3 py-1.5 backdrop-blur-sm">
                                    <span className="text-[11px] font-medium text-text-secondary">
                                        {group.label}
                                    </span>
                                    <Badge tone="neutral" appearance="soft">
                                        {group.items.length}
                                    </Badge>
                                    {!group.isLocal ? (
                                        <span className="truncate text-[10.5px] text-text-tertiary">
                                            仅同机 watch · 不同服务器互不调用
                                        </span>
                                    ) : null}
                                </div>
                                <ul className="divide-y divide-border-subtle/70">
                                    {group.items.map((candidate) => {
                                        const checked = selectedSet.has(candidate.bot_id);
                                        const order = selected.indexOf(candidate.bot_id);
                                        return (
                                            <li
                                                key={candidate.bot_id}
                                                className={cn(
                                                    'group relative',
                                                    checked && 'bg-brand/6',
                                                )}
                                            >
                                                <div className="flex items-stretch gap-0">
                                                    <button
                                                        type="button"
                                                        onClick={() =>
                                                            toggle(candidate.bot_id)
                                                        }
                                                        className="flex min-w-0 flex-1 items-start gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-inset/40"
                                                    >
                                                        <span
                                                            className={cn(
                                                                'mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-xs border text-[10px] font-medium',
                                                                checked
                                                                    ? 'border-brand bg-brand text-white'
                                                                    : 'border-border-subtle bg-field text-transparent',
                                                            )}
                                                            aria-hidden
                                                        >
                                                            {checked && order >= 0
                                                                ? order + 1
                                                                : '✓'}
                                                        </span>
                                                        <div className="min-w-0 flex-1">
                                                            <div className="flex min-w-0 items-center gap-1.5">
                                                                <span className="truncate text-[13px] font-medium text-text">
                                                                    {candidate.name ||
                                                                        candidate.bot_id}
                                                                </span>
                                                                <Badge
                                                                    tone="neutral"
                                                                    appearance="soft"
                                                                >
                                                                    {backendLabel(
                                                                        candidate.backend_type,
                                                                    )}
                                                                </Badge>
                                                            </div>
                                                            <p className="mt-0.5 truncate text-[11px] text-text-tertiary">
                                                                {candidate.bot_id}
                                                                {' · '}
                                                                {stateLabel(candidate.state)}
                                                                {candidate.has_local_http
                                                                    ? ` · :${candidate.http_port || '?'}`
                                                                    : ' · 缺 HTTP'}
                                                            </p>
                                                        </div>
                                                    </button>

                                                    <div className="flex shrink-0 items-center pr-3">
                                                        {statusBadge(candidate)}
                                                    </div>
                                                </div>
                                            </li>
                                        );
                                    })}
                                </ul>
                            </div>
                        ))}
                        {selectedMissing.map((id) => (
                            <div
                                key={`missing-${id}`}
                                className="flex items-center justify-between gap-2 border-t border-border-subtle/70 px-3 py-2.5"
                            >
                                <div className="min-w-0">
                                    <div className="truncate text-[13px] font-medium text-text">
                                        {id}
                                    </div>
                                    <p className="text-[11px] text-text-tertiary">
                                        已保存，但不在当前候选中
                                    </p>
                                </div>
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                        onChange(
                                            selected.filter((item) => item !== id),
                                        )
                                    }
                                >
                                    移除
                                </Button>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </section>
    );
}

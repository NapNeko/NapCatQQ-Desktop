// 通知设置：远端 NCD Watch（按服务器分行，支持多机）。
// 视觉对齐 Webhook 通道列表：SettingsSection + FieldRow，不另起卡片面板。
// 安装 / 同步是即时动作，不进设置草稿。

import { useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { componentService } from '../../../core/services/component.service';
import { ncdWatchService } from '../../../core/services/ncd-watch.service';
import { errorText } from '../../../core/domain/errors';
import { compareSemver } from '../../../core/domain/release/normalize';
import {
    useNcdWatchServers,
    type NcdWatchServerRow,
} from '../../../hooks/settings/useNcdWatchServers';
import { useReleases } from '../../../hooks/diagnostics/useReleases';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { Badge, Button, Spinner } from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';
import { FieldRow, SettingsSection } from '../_shared';

type BusyKind = 'install' | 'update' | 'sync';

function connectionLabel(state: NcdWatchServerRow['state']): string {
    switch (state) {
        case 'connected':
            return '已连接';
        case 'connecting':
            return '连接中';
        case 'failed':
            return '连接失败';
        default:
            return '未连接';
    }
}

function watchStatusLabel(row: NcdWatchServerRow, latestVersion: string | null): string {
    if (row.watchInstalled === null && !row.detectError) return '探测中';
    if (row.watchInstalled) {
        const local = row.watchVersion?.trim() || '';
        if (!local) return '已安装';
        if (latestVersion && compareSemver(local, latestVersion) > 0) {
            return `已安装 · ${local} → ${latestVersion}`;
        }
        return `已安装 · ${local}`;
    }
    if (row.detectError) return '未确认';
    return latestVersion ? `未安装 · 可装 ${latestVersion}` : '未安装';
}

function rowDescription(row: NcdWatchServerRow, latestVersion: string | null): string {
    const bots = row.botCount > 0 ? `${row.botCount} 个 Bot` : '暂无 Bot';
    const parts = [
        row.hostLabel,
        connectionLabel(row.state),
        bots,
        watchStatusLabel(row, latestVersion),
    ];
    if (row.detectError) parts.push(row.detectError);
    return parts.join(' · ');
}

function readyDot(row: NcdWatchServerRow): boolean {
    return row.watchInstalled === true && row.state === 'connected';
}

function canUpdateWatch(row: NcdWatchServerRow, latestVersion: string | null): boolean {
    if (!row.watchInstalled || !latestVersion) return false;
    const local = row.watchVersion?.trim();
    if (!local) return true;
    return compareSemver(local, latestVersion) > 0;
}

export function NcdWatchRemoteSection({
    settingsDirty,
}: {
    settingsDirty: boolean;
}) {
    const { rows, loading, refetchAll } = useNcdWatchServers();
    const { snapshot: releases } = useReleases();
    const latestWatchVersion = releases.ncdWatch?.version ?? null;
    const [busy, setBusy] = useState<Record<string, BusyKind | undefined>>({});
    const [bulkSyncing, setBulkSyncing] = useState(false);

    const setRowBusy = (serverId: string, kind: BusyKind | undefined) => {
        setBusy((prev) => {
            const next = { ...prev };
            if (!kind) delete next[serverId];
            else next[serverId] = kind;
            return next;
        });
    };

    const installWatch = async (row: NcdWatchServerRow) => {
        setRowBusy(row.serverId, 'install');
        try {
            await componentService.runComponentAction(
                'ncd_watch',
                `remote:${row.serverId}`,
                'ensure_installed',
            );
            pushInfoBar({
                key: `ncd-watch-install-${row.serverId}`,
                tone: 'info',
                title: '已提交 NCD Watch 安装',
                content: `${row.name}：进度见任务队列；装完后可点「同步」。`,
            });
            window.setTimeout(() => refetchAll(), 2500);
        } catch (err) {
            pushInfoBar({
                key: `ncd-watch-install-${row.serverId}`,
                tone: 'danger',
                title: '安装失败',
                content: `${row.name}：${errorText(err, '未知错误')}`,
            });
        } finally {
            setRowBusy(row.serverId, undefined);
        }
    };

    const updateWatch = async (row: NcdWatchServerRow) => {
        setRowBusy(row.serverId, 'update');
        try {
            await componentService.runComponentAction(
                'ncd_watch',
                `remote:${row.serverId}`,
                'update',
            );
            pushInfoBar({
                key: `ncd-watch-update-${row.serverId}`,
                tone: 'info',
                title: '已提交 NCD Watch 更新',
                content: `${row.name}：将拉取最新二进制并重启服务；进度见任务队列。`,
            });
            window.setTimeout(() => refetchAll(), 2500);
        } catch (err) {
            pushInfoBar({
                key: `ncd-watch-update-${row.serverId}`,
                tone: 'danger',
                title: '更新失败',
                content: `${row.name}：${errorText(err, '未知错误')}`,
            });
        } finally {
            setRowBusy(row.serverId, undefined);
        }
    };

    const syncOne = async (row: NcdWatchServerRow) => {
        if (settingsDirty) {
            pushInfoBar({
                key: 'ncd-watch-sync-dirty',
                tone: 'warning',
                title: '请先保存设置',
                content: '同步使用已落盘的通知设置（Webhook / Email / 同机 OneBot）；保存后再同步。',
            });
            return;
        }
        setRowBusy(row.serverId, 'sync');
        try {
            await ncdWatchService.syncNotify(row.serverId);
            pushInfoBar({
                key: `ncd-watch-sync-${row.serverId}`,
                tone: 'success',
                title: '已同步',
                content: `${row.name}：Bot 列表、Webhook/Email/同机 OneBot 与登录探活凭据已写入远端。`,
            });
            refetchAll();
        } catch (err) {
            pushInfoBar({
                key: `ncd-watch-sync-${row.serverId}`,
                tone: 'danger',
                title: '同步失败',
                content: `${row.name}：${errorText(err, '未知错误')}`,
            });
        } finally {
            setRowBusy(row.serverId, undefined);
        }
    };

    const syncAll = async () => {
        if (settingsDirty) {
            pushInfoBar({
                key: 'ncd-watch-sync-dirty',
                tone: 'warning',
                title: '请先保存设置',
                content: '批量同步使用已保存的通知设置（Webhook / Email / 同机 OneBot）。',
            });
            return;
        }
        if (rows.length === 0) return;
        setBulkSyncing(true);
        let ok = 0;
        let fail = 0;
        for (const row of rows) {
            setRowBusy(row.serverId, 'sync');
            try {
                await ncdWatchService.syncNotify(row.serverId);
                ok += 1;
            } catch {
                fail += 1;
            } finally {
                setRowBusy(row.serverId, undefined);
            }
        }
        setBulkSyncing(false);
        refetchAll();
        pushInfoBar({
            key: 'ncd-watch-sync-all',
            tone: fail === 0 ? 'success' : ok > 0 ? 'warning' : 'danger',
            title: '批量同步结束',
            content: `成功 ${ok} 台${fail > 0 ? `，失败 ${fail} 台` : ''}。`,
        });
    };

    const installedCount = rows.filter((r) => r.watchInstalled).length;
    const updateCount = rows.filter((r) => canUpdateWatch(r, latestWatchVersion)).length;
    const anyBusy = bulkSyncing || Object.values(busy).some(Boolean);

    return (
        <SettingsSection
            title="远端脱管监控"
            description="Desktop 退出后，各 Linux 远端上的 NCD Watch 仍可探活并投递已启用的 Webhook / Email / 同机 OneBot（需同机另有存活发信 Bot）。Webhook URL 在远端解析：本机 127.0.0.1 测服需 SSH 反向隧道或公网/可达地址；多机各自安装与同步"
        >
            <FieldRow
                label="远端主机"
                description={
                    settingsDirty
                        ? '设置未保存：同步会用磁盘上的旧通知配置，请先保存'
                        : loading && rows.length === 0
                            ? '正在加载远端主机…'
                            : rows.length === 0
                                ? '还没有远端服务器；先在「远程」页添加'
                                : installedCount > 0
                                    ? `${rows.length} 台主机 · ${installedCount} 台已装 Watch${updateCount > 0
                                        ? ` · ${updateCount} 台可更新${latestWatchVersion
                                            ? `（${latestWatchVersion}）`
                                            : ''
                                        }`
                                        : latestWatchVersion
                                            ? ` · 最新 ${latestWatchVersion}`
                                            : ''
                                    }`
                                    : `${rows.length} 台主机 · 尚未安装 Watch${latestWatchVersion
                                        ? ` · 可装 ${latestWatchVersion}`
                                        : ''
                                    }`
                }
                isLast={rows.length === 0}
            >
                <div className="flex items-center gap-1.5">
                    {loading && rows.length === 0 ? <Spinner size="xs" /> : null}
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={loading || anyBusy}
                        aria-label="刷新远端 Watch 状态"
                        onClick={() => refetchAll()}
                    >
                        <ActionMotionIcon icon={RefreshCw} size={14} />
                    </Button>
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        disabled={
                            loading ||
                            anyBusy ||
                            rows.length === 0 ||
                            settingsDirty
                        }
                        onClick={() => void syncAll()}
                    >
                        {bulkSyncing ? '同步中…' : '全部同步'}
                    </Button>
                </div>
            </FieldRow>

            {rows.map((row, index) => {
                const rowBusy = busy[row.serverId];
                const syncDisabled =
                    !!rowBusy ||
                    bulkSyncing ||
                    settingsDirty ||
                    row.state === 'connecting';
                const actionDisabled = !!rowBusy || bulkSyncing;
                const showUpdate = canUpdateWatch(row, latestWatchVersion);
                return (
                    <FieldRow
                        key={row.serverId}
                        label={row.name}
                        description={rowDescription(row, latestWatchVersion)}
                        isLast={index === rows.length - 1}
                    >
                        <div className="flex items-center gap-1.5">
                            <span
                                className={cn(
                                    'mr-1 h-1.5 w-1.5 shrink-0 rounded-full',
                                    readyDot(row)
                                        ? 'bg-success'
                                        : 'bg-text-tertiary/45',
                                )}
                                aria-hidden
                            />
                            {row.watchInstalled === false ? (
                                <Badge tone="neutral" appearance="soft">
                                    未安装
                                </Badge>
                            ) : showUpdate ? (
                                <Badge tone="warning" appearance="soft">
                                    可更新
                                </Badge>
                            ) : row.state === 'failed' || row.detectError ? (
                                <Badge tone="warning" appearance="soft">
                                    {row.state === 'failed' ? '连接失败' : '异常'}
                                </Badge>
                            ) : null}
                            {row.watchInstalled ? (
                                showUpdate ? (
                                    <Button
                                        type="button"
                                        variant="primary"
                                        size="sm"
                                        disabled={actionDisabled}
                                        onClick={() => void updateWatch(row)}
                                    >
                                        {rowBusy === 'update' ? '提交中…' : '更新'}
                                    </Button>
                                ) : (
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        disabled={actionDisabled}
                                        onClick={() => void installWatch(row)}
                                    >
                                        {rowBusy === 'install' ? '提交中…' : '重装'}
                                    </Button>
                                )
                            ) : (
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    disabled={actionDisabled}
                                    onClick={() => void installWatch(row)}
                                >
                                    {rowBusy === 'install' ? '提交中…' : '安装'}
                                </Button>
                            )}
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                disabled={syncDisabled}
                                onClick={() => void syncOne(row)}
                            >
                                {rowBusy === 'sync' ? '同步中…' : '同步'}
                            </Button>
                        </div>
                    </FieldRow>
                );
            })}
        </SettingsSection>
    );
}

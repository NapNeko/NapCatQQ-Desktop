// 关于 Tab。版本信息 + 检查更新 + 许可。
//
// 本地版本来自 BootstrapSnapshot.local_versions（NapCat / SnowLuma 已安装版本）。
// 远端最新来自 useReleases（GitHub releases 快照，1h TTL 缓存）。两者比对得出
// "有更新可用"，给出跳转 GitHub release 页的入口。检查更新 = 强制 refetch。

import { ExternalLink, RefreshCw } from 'lucide-react';
import type { LocalVersionSnapshot } from '../../../core/ipc/types';
import { useReleases } from '../../../hooks/diagnostics/useReleases';
import { useOpenExternal } from '../../../hooks/desktop/useOpenExternal';
import {
    findUpdatesAvailable,
    type ReleaseInfoView,
} from '../../../core/domain/release/normalize';
import { Button } from '../../../shared/ui';
import { FieldRow } from '../_shared';

const APP_VERSION = '0.1.0-alpha.1';

interface Props {
    localVersions: LocalVersionSnapshot | null;
}

export function AboutTab({ localVersions }: Props) {
    const { snapshot, isFetching, refetch } = useReleases();
    const openExternal = useOpenExternal();

    const local = localVersions ?? { napcat: null, snowluma: null };
    const updates = findUpdatesAvailable(local, snapshot);
    const hasNapcatUpdate = updates.some((u) => u.project === 'napcat');
    const hasSnowlumaUpdate = updates.some((u) => u.project === 'snowluma');

    return (
        <>
            <FieldRow label="NapCatQQ Desktop" description="桌面端版本">
                <div className="flex items-center gap-2">
                    <span className="font-mono text-[12.5px] text-text-secondary">
                        {APP_VERSION}
                    </span>
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => refetch()}
                        disabled={isFetching}
                    >
                        <RefreshCw
                            size={13}
                            className={isFetching ? 'animate-spin' : undefined}
                        />
                        <span>{isFetching ? '检查中…' : '检查更新'}</span>
                    </Button>
                </div>
            </FieldRow>

            <VersionRow
                label="NapCat 内核"
                localVersion={local.napcat ?? null}
                remote={snapshot.napcat}
                hasUpdate={hasNapcatUpdate}
                onOpen={openExternal}
            />

            <VersionRow
                label="SnowLuma 内核"
                localVersion={local.snowluma ?? null}
                remote={snapshot.snowluma}
                hasUpdate={hasSnowlumaUpdate}
                onOpen={openExternal}
            />

            <FieldRow
                label="许可"
                description="本项目以 GPL-3.0 协议开源，详见仓库 LICENSE"
                isLast
            >
                <span className="font-mono text-[12px] text-text-tertiary">GPL-3.0</span>
            </FieldRow>
        </>
    );
}

/// 单个内核的版本行：本地版 vs 远端最新，有更新时高亮 + 给跳转入口。
function VersionRow({
    label,
    localVersion,
    remote,
    hasUpdate,
    onOpen,
}: {
    label: string;
    localVersion: string | null;
    remote: ReleaseInfoView | null;
    hasUpdate: boolean;
    onOpen: (url: string) => void;
}) {
    const localText = localVersion ?? '未安装';
    const description = remote
        ? hasUpdate
            ? `当前 ${localText} · 最新 ${remote.version}`
            : `当前 ${localText} · 已是最新`
        : `当前 ${localText} · 远端版本获取中`;

    return (
        <FieldRow label={label} description={description}>
            {hasUpdate && remote ? (
                <Button variant="secondary" size="sm" onClick={() => onOpen(remote.htmlUrl)}>
                    <span className="text-brand">有新版</span>
                    <ExternalLink size={13} />
                </Button>
            ) : remote ? (
                <Button variant="ghost" size="sm" onClick={() => onOpen(remote.htmlUrl)}>
                    <ExternalLink size={13} />
                </Button>
            ) : (
                <span className="font-mono text-[12px] text-text-tertiary">—</span>
            )}
        </FieldRow>
    );
}

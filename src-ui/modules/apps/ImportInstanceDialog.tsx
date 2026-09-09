// 导入已有项目：先看目录，再原地接管。

import React, { useEffect, useState } from 'react';
import { FolderOpen, Import } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
    Spinner,
    TextField,
    type SelectItem,
} from '../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../shared/ui/motion';
import type { useServerManager } from '../../hooks/remote/useServerManager';
import { errorText } from '../../core/domain/errors';
import { pickDirectory } from '../../core/ipc/transport';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { dismissInfoBar, pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { RemoteDirectoryPicker } from '../../shared/components/RemoteDirectoryPicker';
import { remoteServerIdFromHostId } from '../../core/domain/remote-host/posixPath';
import { hostIdDisplayLabel } from './hostLabel';
import type { AppFrameworkManifest, AppProjectProbe } from '../../core/ipc/types';

const PROBE_ERROR_KEY = 'app-probe';
const PROBE_WARN_KEY = 'app-probe-warn';

function clearProbeBars() {
    dismissInfoBar(`key:${PROBE_ERROR_KEY}`);
    dismissInfoBar(`key:${PROBE_WARN_KEY}`);
}

export interface ImportInstanceTarget {
    /** 固定安装位置（组件页按主机导入时传） */
    lockedHostId?: string;
    /** 固定框架（组件页按框架导入时传） */
    manifest?: AppFrameworkManifest;
}

type Servers = ReturnType<typeof useServerManager>['servers'];

export const ImportInstanceDialog: React.FC<{
    target: ImportInstanceTarget | null;
    frameworks: AppFrameworkManifest[];
    servers: Servers;
    isImporting: boolean;
    onClose: () => void;
    onSubmit: (args: {
        frameworkId: string;
        hostId: string;
        path: string;
        displayName: string;
    }) => Promise<void>;
}> = ({ target, frameworks, servers, isImporting, onClose, onSubmit }) => {
    const [frameworkId, setFrameworkId] = useState('');
    const [hostId, setHostId] = useState('local');
    const [path, setPath] = useState('');
    const [displayName, setDisplayName] = useState('');
    const [probe, setProbe] = useState<AppProjectProbe | null>(null);
    const [probing, setProbing] = useState(false);
    const [pickerOpen, setPickerOpen] = useState(false);
    const [mounted, setMounted] = useState<ImportInstanceTarget | null>(null);

    useEffect(() => {
        if (!target) return;
        setMounted(target);
        const first = target.manifest ?? frameworks[0];
        setFrameworkId(first?.id ?? '');
        setHostId(target.lockedHostId ?? 'local');
        setPath('');
        setDisplayName('');
        setProbe(null);
        setPickerOpen(false);
        clearProbeBars();
    }, [target, frameworks]);

    const manifest =
        mounted?.manifest ?? frameworks.find((m) => m.id === frameworkId) ?? null;
    const lockedHost = mounted?.lockedHostId ?? null;
    const lockedFramework = mounted?.manifest != null;
    const isRemote = hostId.startsWith('remote:');
    const remoteId = remoteServerIdFromHostId(hostId);
    const remoteHome = servers.find((s) => s.id === remoteId)?.inventory?.home;
    const remotePathInvalid = isRemote && path.trim() !== '' && !path.trim().startsWith('/');
    const canProbe = Boolean(frameworkId && path.trim() && !probing);

    const supportsLocal = manifest?.supported_placements.includes('local_native') ?? false;
    const supportsRemote = manifest?.supported_placements.includes('remote_native') ?? false;
    const hostItems: SelectItem[] = lockedHost
        ? [{ value: lockedHost, label: hostIdDisplayLabel(lockedHost, servers) }]
        : [
              ...(supportsLocal ? [{ value: 'local', label: '本机' }] : []),
              ...(supportsRemote
                  ? servers.map((s) => ({
                        value: `remote:${s.id}`,
                        label: s.name?.trim() || s.host || s.id,
                    }))
                  : []),
          ];
    const frameworkItems: SelectItem[] = frameworks.map((m) => ({
        value: m.id,
        label: m.display_name,
    }));

    const runProbe = async () => {
        if (!canProbe) return;
        if (remotePathInvalid) {
            pushErrorBar({
                key: PROBE_ERROR_KEY,
                title: '检查项目失败',
                content: '远端路径要以 / 开头',
            });
            return;
        }
        setProbing(true);
        setProbe(null);
        clearProbeBars();
        try {
            const next = await appFrameworkService.probeProject(hostId, frameworkId, path.trim());
            setProbe(next);
            if (!displayName.trim()) setDisplayName(next.display_name);
            const notes = next.warnings.filter((w) => !w.includes('快照'));
            if (notes.length > 0) {
                pushInfoBar({
                    key: PROBE_WARN_KEY,
                    tone: 'warning',
                    title: notes[0],
                    content: notes.slice(1).join(' ') || undefined,
                });
            }
        } catch (e) {
            pushErrorBar({
                key: PROBE_ERROR_KEY,
                title: '检查项目失败',
                raw: errorText(e),
            });
        } finally {
            setProbing(false);
        }
    };

    const markPathDirty = (next: string) => {
        setPath(next);
        setProbe(null);
        clearProbeBars();
    };

    return (
        <>
        <Dialog open={target !== null} onOpenChange={(o) => !o && !isImporting && !pickerOpen && onClose()}>
            <DialogContent size="md" dismissOnOutsideClick={!isImporting && !pickerOpen} onExited={() => setMounted(null)}>
                {mounted && (
                    <>
                        <DialogHeader>
                            <DialogTitle>
                                导入{manifest ? ` ${manifest.display_name}` : ''} 项目
                            </DialogTitle>
                            <DialogDescription>目录还在原地，不会复制，也不会改入口。</DialogDescription>
                        </DialogHeader>
                        <div className="flex flex-col gap-3">
                            <div className="grid grid-cols-2 gap-3">
                                <Select
                                    label="框架"
                                    items={frameworkItems}
                                    value={frameworkId}
                                    onValueChange={(v) => {
                                        setFrameworkId(v);
                                        setProbe(null);
                                        clearProbeBars();
                                    }}
                                    disabled={lockedFramework}
                                />
                                <Select
                                    label="主机"
                                    items={hostItems}
                                    value={hostId}
                                    onValueChange={(v) => {
                                        setHostId(v);
                                        setPath('');
                                        setProbe(null);
                                        clearProbeBars();
                                    }}
                                    disabled={lockedHost !== null}
                                />
                            </div>
                            <div className="flex flex-col gap-1.5">
                                <span className="text-xs font-medium text-text-secondary">项目目录</span>
                                <div className="flex items-center gap-1.5">
                                    <TextField
                                        className="min-w-0 flex-1"
                                        aria-label="项目目录"
                                        placeholder={isRemote ? '/root/my-bot' : undefined}
                                        value={path}
                                        onValueChange={markPathDirty}
                                        onKeyDown={(e) => {
                                            if (e.key === 'Enter') {
                                                e.preventDefault();
                                                void runProbe();
                                            }
                                        }}
                                    />
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="md"
                                        className="shrink-0"
                                        disabled={isImporting || (isRemote && !remoteId)}
                                        onClick={() => {
                                            if (isRemote) {
                                                setPickerOpen(true);
                                                return;
                                            }
                                            void pickDirectory('选择已有项目目录').then((dir) => {
                                                if (!dir) return;
                                                markPathDirty(dir);
                                            });
                                        }}
                                    >
                                        <FolderOpen size={14} strokeWidth={2.2} />
                                        选择
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="md"
                                        className="shrink-0"
                                        disabled={!canProbe}
                                        onClick={() => void runProbe()}
                                    >
                                        {probing && <Spinner size="sm" />}
                                        检查
                                    </Button>
                                </div>
                            </div>
                            <TextField
                                label="实例名"
                                placeholder={probe?.display_name || '留空则用项目名'}
                                value={displayName}
                                onValueChange={setDisplayName}
                            />
                            {probe && <ProbePreview probe={probe} />}
                        </div>
                        <DialogFooter>
                            <Button variant="ghost" size="sm" onClick={onClose} disabled={isImporting}>
                                取消
                            </Button>
                            <Button
                                variant="primary"
                                size="sm"
                                disabled={isImporting || !probe || probing}
                                onClick={() =>
                                    void onSubmit({
                                        frameworkId,
                                        hostId,
                                        path: probe?.path || path.trim(),
                                        displayName,
                                    }).catch(() => undefined)
                                }
                            >
                                {isImporting ? (
                                    <Spinner size="sm" className="text-white" />
                                ) : (
                                    <ActionMotionIcon
                                        icon={Import}
                                        size={14}
                                        strokeWidth={2.4}
                                        motion={EMPHASIS_MOTION}
                                    />
                                )}
                                导入
                            </Button>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
        <RemoteDirectoryPicker
            open={pickerOpen && !!remoteId}
            remoteId={remoteId}
            initialPath={path}
            initialRoot={remoteHome ?? '/'}
            onClose={() => setPickerOpen(false)}
            onSelect={(next) => {
                markPathDirty(next);
                setPickerOpen(false);
            }}
        />
        </>
    );
};

const ProbePreview: React.FC<{ probe: AppProjectProbe }> = ({ probe }) => {
    const env =
        probe.environment && probe.environment !== probe.env_rel_path
            ? `${probe.env_rel_path} · ${probe.environment}`
            : probe.env_rel_path;
    const status = probe.running ? '正在运行' : probe.ready ? '可以启动' : '依赖还没齐';
    const rows: [string, string][] = [
        ...(probe.port != null ? ([['端口', String(probe.port)]] as [string, string][]) : []),
        ['配置', env],
        ['状态', status],
        ...(probe.detected_bot_id
            ? ([['对接', `Bot ${probe.detected_bot_id}`]] as [string, string][])
            : []),
    ];

    return (
        <div className="flex flex-col gap-2 rounded-md border border-border-subtle bg-inset/40 px-3 py-2.5">
            <div className="flex items-baseline justify-between gap-3">
                <p className="text-sm text-text">{probe.display_name}</p>
                {probe.version && (
                    <p className="shrink-0 text-2xs text-text-tertiary">v{probe.version}</p>
                )}
            </div>
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
                {rows.map(([k, v]) => (
                    <React.Fragment key={k}>
                        <dt className="text-text-tertiary">{k}</dt>
                        <dd className="min-w-0 text-text">{v}</dd>
                    </React.Fragment>
                ))}
            </dl>
        </div>
    );
};

export default ImportInstanceDialog;

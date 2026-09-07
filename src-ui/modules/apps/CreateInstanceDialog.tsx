// 新建应用实例对话框：组件页（按主机装）与应用端页共用。
// 传 lockedHostId 时安装位置固定为该主机（组件页是主机主导视图）。

import React, { useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    NumberField,
    Select,
    Spinner,
    TextField,
    type SelectItem,
} from '../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../shared/ui/motion';
import type { useServerManager } from '../../hooks/remote/useServerManager';
import { hostIdDisplayLabel } from './hostLabel';
import type { AppFrameworkManifest } from '../../core/ipc/types';

export interface CreateInstanceDraft {
    frameworkId: string;
    hostId: string;
    displayName: string;
    port: number | null;
    installNow: boolean;
}

export interface CreateInstanceRequest {
    manifest: AppFrameworkManifest;
    /** 固定安装位置（组件页按主机新建时传） */
    lockedHostId?: string;
}

type Servers = ReturnType<typeof useServerManager>['servers'];

export const CreateInstanceDialog: React.FC<{
    request: CreateInstanceRequest | null;
    servers: Servers;
    isCreating: boolean;
    onClose: () => void;
    onSubmit: (draft: CreateInstanceDraft) => Promise<void>;
}> = ({ request, servers, isCreating, onClose, onSubmit }) => {
    const [draft, setDraft] = useState<CreateInstanceDraft | null>(null);
    // 关闭动画期间保留内容，避免对话框在退场时先变空
    const [mounted, setMounted] = useState<CreateInstanceRequest | null>(null);

    useEffect(() => {
        if (!request) return;
        setMounted(request);
        setDraft({
            frameworkId: request.manifest.id,
            hostId: request.lockedHostId ?? 'local',
            displayName: '',
            port: request.manifest.default_port,
            installNow: true,
        });
    }, [request]);

    const manifest = mounted?.manifest ?? null;
    const locked = mounted?.lockedHostId ?? null;
    const supportsLocal = manifest?.supported_placements.includes('local_native') ?? false;
    const supportsRemote = manifest?.supported_placements.includes('remote_native') ?? false;

    const hostItems: SelectItem[] = locked
        ? [{ value: locked, label: hostIdDisplayLabel(locked, servers) }]
        : [
              ...(supportsLocal ? [{ value: 'local', label: '本机' }] : []),
              ...(supportsRemote
                  ? servers.map((s) => ({
                        value: `remote:${s.id}`,
                        label: s.name?.trim() || s.host || s.id,
                    }))
                  : []),
          ];

    const portInvalid = draft?.port != null && (draft.port < 1 || draft.port > 65535);

    return (
        <Dialog open={request !== null} onOpenChange={(o) => !o && !isCreating && onClose()}>
            <DialogContent size="md" dismissOnOutsideClick={!isCreating} onExited={() => setMounted(null)}>
                {manifest && draft && (
                    <>
                        <DialogHeader>
                            <DialogTitle>新建 {manifest.display_name} 实例</DialogTitle>
                            <DialogDescription>
                                {locked ? '安装到当前选中的主机' : '选择安装位置'}
                            </DialogDescription>
                        </DialogHeader>
                        <div className="flex flex-col gap-3">
                            <Select
                                label="安装位置"
                                items={hostItems}
                                value={draft.hostId}
                                onValueChange={(v) => setDraft({ ...draft, hostId: v })}
                                disabled={locked !== null}
                                hint={
                                    !locked && supportsRemote && servers.length === 0
                                        ? '尚未添加远端主机，可先到「远端」页添加'
                                        : draft.hostId.startsWith('remote:')
                                          ? '远端经 SSH 安装、启停与写配置'
                                          : undefined
                                }
                            />
                            <TextField
                                label="实例名"
                                placeholder={`${manifest.display_name} · ${hostIdDisplayLabel(draft.hostId, servers)}`}
                                value={draft.displayName}
                                onValueChange={(v) => setDraft({ ...draft, displayName: v })}
                                hint="留空则自动命名"
                            />
                            <NumberField
                                label="监听端口"
                                value={draft.port}
                                onValueChange={(v) => setDraft({ ...draft, port: v })}
                                error={portInvalid ? '端口需在 1–65535 之间' : undefined}
                                hint="留空则取默认端口，并自动避让同机已有实例"
                            />
                            <Checkbox
                                label="创建后立即安装"
                                hint={
                                    manifest.runtime_component_ids.length > 0
                                        ? '缺少的运行时依赖会一并安装'
                                        : undefined
                                }
                                checked={draft.installNow}
                                onCheckedChange={(c) => setDraft({ ...draft, installNow: c })}
                            />
                        </div>
                        <DialogFooter>
                            <Button variant="ghost" size="sm" onClick={onClose} disabled={isCreating}>
                                取消
                            </Button>
                            <Button
                                variant="primary"
                                size="sm"
                                disabled={isCreating || !draft.hostId || portInvalid}
                                onClick={() => void onSubmit(draft).catch(() => undefined)}
                            >
                                {isCreating ? (
                                    <Spinner size="sm" className="text-white" />
                                ) : (
                                    <ActionMotionIcon icon={Plus} size={14} strokeWidth={2.4} motion={EMPHASIS_MOTION} />
                                )}
                                创建
                            </Button>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
    );
};

export default CreateInstanceDialog;

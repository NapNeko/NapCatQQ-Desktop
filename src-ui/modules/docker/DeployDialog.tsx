// 部署对话框：容器名 + 端口（默认值可改）+ NapCat 可选 QQ 号。
// 提交前用 domain 的 validateDeploySpec 做即时校验。
// 确认后由调用方关弹窗并在后台部署；本对话框不再展示进行中 / 完成态。

import React, { useMemo, useState } from 'react';
import { Plus, X } from 'lucide-react';
import { ActionMotionIcon } from '../../shared/ui/motion';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    Button,
    TextField,
    NumberField,
} from '../../shared/ui';
import { validateDeploySpec, portPurpose } from '../../core/domain/docker/spec';
import { errorText } from '../../core/domain/errors';
import type { DockerDeploySpec, DockerFlavor, PortMapping } from '../../core/ipc/types';

interface DeployDialogProps {
    flavor: DockerFlavor;
    initialSpec: DockerDeploySpec;
    /** 保留兼容；表单提交后由调用方关窗，不再锁对话框 */
    isDeploying?: boolean;
    taskId: string;
    onClose: () => void;
    onConfirm: (spec: DockerDeploySpec) => void | Promise<void>;
}

export const DeployDialog: React.FC<DeployDialogProps> = ({
    flavor,
    initialSpec,
    onClose,
    onConfirm,
}) => {
    const [name, setName] = useState(initialSpec.containerName);
    const [ports, setPorts] = useState<PortMapping[]>(initialSpec.ports);
    const [qqId, setQqId] = useState<number | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [submitting, setSubmitting] = useState(false);

    const presetContainerPorts = useMemo(
        () => new Set(initialSpec.ports.map((p) => p.container)),
        [initialSpec.ports],
    );

    const title = flavor === 'napcat' ? '部署 NapCat' : '部署 SnowLuma';

    const setHostPort = (index: number, host: number | null) => {
        setPorts((prev) =>
            prev.map((p, i) => (i === index ? { ...p, host: host ?? p.host } : p)),
        );
    };

    const setContainerPort = (index: number, container: number | null) => {
        setPorts((prev) =>
            prev.map((p, i) => (i === index ? { ...p, container: container ?? p.container } : p)),
        );
    };

    const addPort = () => {
        const used = new Set(ports.map((p) => p.host));
        let candidate = 8080;
        while (used.has(candidate)) candidate += 1;
        setPorts((prev) => [...prev, { host: candidate, container: candidate }]);
    };

    const removePort = (index: number) => {
        setPorts((prev) => prev.filter((_, i) => i !== index));
    };

    const handleConfirm = async () => {
        const spec: DockerDeploySpec = {
            flavor,
            containerName: name.trim(),
            ports,
            qqId: flavor === 'napcat' && qqId ? BigInt(qqId) : null,
        };
        const err = validateDeploySpec(spec);
        if (err) {
            setError(err);
            return;
        }
        setError(null);
        setSubmitting(true);
        try {
            await onConfirm(spec);
        } catch (e) {
            setError(errorText(e, '无法开始部署，请稍后重试'));
            setSubmitting(false);
        }
    };

    return (
        <Dialog open onOpenChange={(o) => { if (!o && !submitting) onClose(); }}>
            <DialogContent size="lg">
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>
                        左侧是宿主机端口（可改），右侧是容器内端口。默认端口已填好，
                        冲突就改宿主机端口；也可「添加端口」映射自定义服务。提交后可在任务队列查看进度。
                    </DialogDescription>
                </DialogHeader>

                <div className="-mx-1 flex max-h-[60vh] flex-col gap-4 overflow-y-auto px-1 py-1">
                    <TextField
                        label="容器名"
                        required
                        value={name}
                        onValueChange={setName}
                        placeholder={flavor === 'napcat' ? 'napcat' : 'snowluma'}
                    />

                    <div className="flex flex-col gap-2">
                        <div className="flex items-center justify-between">
                            <p className="text-xs font-medium text-text-secondary">端口映射</p>
                            <Button size="sm" variant="ghost" onClick={addPort}>
                                <ActionMotionIcon icon={Plus} size={13} />
                                添加端口
                            </Button>
                        </div>
                        <div className="flex flex-col gap-2.5">
                            {ports.map((p, i) => (
                                <PortRow
                                    key={`${p.container}-${i}`}
                                    mapping={p}
                                    preset={presetContainerPorts.has(p.container)}
                                    onHostChange={(v) => setHostPort(i, v)}
                                    onContainerChange={(v) => setContainerPort(i, v)}
                                    onRemove={() => removePort(i)}
                                />
                            ))}
                            {ports.length === 0 && (
                                <p className="rounded-sm bg-inset/40 px-2.5 py-3 text-center text-2xs text-text-tertiary">
                                    至少保留一个端口映射，否则容器内服务无法从宿主机访问
                                </p>
                            )}
                        </div>
                    </div>

                    {flavor === 'napcat' && (
                        <NumberField
                            label="预绑 QQ 号（可选）"
                            hint="填了会作为 ACCOUNT 传入，启动时自动定位该账号"
                            value={qqId}
                            onValueChange={setQqId}
                        />
                    )}

                    {error && <p className="text-xs text-danger">{error}</p>}
                </div>

                <DialogFooter>
                    <Button variant="ghost" onClick={onClose} disabled={submitting}>
                        取消
                    </Button>
                    <Button onClick={() => void handleConfirm()} disabled={submitting}>
                        开始部署
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};

const PortRow: React.FC<{
    mapping: PortMapping;
    preset: boolean;
    onHostChange: (host: number | null) => void;
    onContainerChange: (container: number | null) => void;
    onRemove: () => void;
}> = ({ mapping, preset, onHostChange, onContainerChange, onRemove }) => {
    const purpose = preset ? portPurpose(mapping.container) : null;
    return (
        <div className="flex items-center gap-3 rounded-sm bg-inset/40 px-2.5 py-2">
            <div className="min-w-0 flex-1">
                {preset ? (
                    <>
                        <div className="flex items-center gap-1.5">
                            <span className="text-xs font-medium text-text">
                                {purpose?.label ?? `端口 ${mapping.container}`}
                            </span>
                            <span className="font-mono text-2xs text-text-tertiary">
                                :{mapping.container}
                            </span>
                        </div>
                        {purpose?.description && (
                            <p className="mt-0.5 text-2xs leading-snug text-text-tertiary">
                                {purpose.description}
                            </p>
                        )}
                    </>
                ) : (
                    <div className="flex items-center gap-1.5">
                        <span className="shrink-0 text-2xs text-text-tertiary">容器端口</span>
                        <NumberField
                            className="w-20"
                            label={undefined}
                            aria-label="容器端口"
                            value={mapping.container}
                            onValueChange={onContainerChange}
                        />
                    </div>
                )}
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
                <NumberField
                    className="w-24"
                    label={undefined}
                    aria-label={`宿主机端口(容器 ${mapping.container})`}
                    value={mapping.host}
                    onValueChange={onHostChange}
                />
                <span className="shrink-0 text-2xs text-text-tertiary">宿主机</span>
                {!preset && (
                    <button
                        type="button"
                        onClick={onRemove}
                        aria-label="删除端口映射"
                        className="shrink-0 rounded-xs p-1 text-text-tertiary transition-colors hover:bg-danger-soft hover:text-danger"
                    >
                        <ActionMotionIcon icon={X} size={14} />
                    </button>
                )}
            </div>
        </div>
    );
};
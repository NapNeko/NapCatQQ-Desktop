// 部署对话框：容器名 + 端口（默认值可改）+ NapCat 可选 QQ 号。
// 提交前用 domain 的 validateDeploySpec 做即时校验。
// 三态切换：表单 → 部署中进度 → 完成结果卡片。进度态隐藏表单显示来自
// dockerDeployProgressStore 的实时进度；成功后不立刻关闭，留在弹窗里展示
// WebUI 地址 + 凭据（凭据仅一次性展示），用户看完点「完成」再关。

import React, { useMemo, useState } from 'react';
import { Plus, RefreshCw, X } from 'lucide-react';
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
import { useDockerDeployProgress } from '../../hooks/docker/useDockerDeployProgress';
import { ProgressLine } from '../components/progressView';
import { DeployResultBody } from './DeployResultBanner';
import type {
    DeployedContainer,
    DockerDeploySpec,
    DockerFlavor,
    PortMapping,
} from '../../core/ipc/types';

// docker_deploy 的 5 步固定标题。用步骤号映射,避免把后端 message(拉取阶段是
// "已完成 X/Y 层")塞进步骤行——那会和下方 ProgressLine 的层数重复。
const STEP_TITLES = ['探测 Docker', '准备部署目录', '拉取镜像', '启动容器', '读取部署结果'];

interface DeployDialogProps {
    flavor: DockerFlavor;
    initialSpec: DockerDeploySpec;
    isDeploying: boolean;
    taskId: string;
    onClose: () => void;
    // 下发部署并返回结果。dialog 拿到结果后切到完成态展示，不立刻关闭。
    onConfirm: (spec: DockerDeploySpec) => Promise<DeployedContainer>;
}

export const DeployDialog: React.FC<DeployDialogProps> = ({
    flavor,
    initialSpec,
    isDeploying,
    taskId,
    onClose,
    onConfirm,
}) => {
    const [name, setName] = useState(initialSpec.containerName);
    const [ports, setPorts] = useState<PortMapping[]>(initialSpec.ports);
    const [qqId, setQqId] = useState<number | null>(null);
    const [error, setError] = useState<string | null>(null);
    // 部署成功的结果。非 null 时弹窗切到完成态展示 WebUI/凭据。
    const [deployed, setDeployed] = useState<DeployedContainer | null>(null);

    const progress = useDockerDeployProgress(taskId);

    // 预置端口（来自镜像默认）的容器端口锁定，只改宿主机；用户自己加的行两端
    // 都能填、可删。用初始 spec 的容器端口集合区分两者。
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
        // 新行默认 host=container=同一个未占用端口（从 8080 起找空位）。
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
        try {
            // 成功后切到完成态展示结果(WebUI/凭据),不立刻关闭——凭据只展示一次,
            // 关早了用户看不到。失败时就地显示后端原因(拉镜像失败 / docker 未就绪 /
            // 端口占用等),对话框保持打开让用户改参数重试。
            const result = await onConfirm(spec);
            setDeployed(result);
        } catch (e) {
            setError(errorText(e, '部署失败，请稍后重试'));
        }
    };

    // 步骤指示文字：用固定的步骤标题(探测/准备/拉取/启动/读取),不复用后端
    // message——后端拉取阶段的 message 是"已完成 X/Y 层",会和下方 ProgressLine
    // 的层数计数重复。层数只在 ProgressLine 一处显示。
    const stepHint =
        progress && progress.totalSteps > 0
            ? `步骤 ${progress.currentStep}/${progress.totalSteps} · ${STEP_TITLES[progress.currentStep - 1] ?? '处理中'}`
            : '正在部署，请稍候…';

    // 最近几条 log，取末尾 4 条，docker pull 的 layer 状态行会在这里滚动。
    const recentLogs = progress ? progress.logs.slice(-4) : [];

    return (
        // 部署进行中锁死对话框：点遮罩 / 按 Esc / 关闭按钮都不放行，否则一点外部
        // 就把对话框连带 taskId 卸载，再开是新 task，进度回不去。部署结束(成功由
        // 调用方主动关、失败 isDeploying 转 false)后恢复正常关闭。
        <Dialog open onOpenChange={(o) => { if (!o && !isDeploying) onClose(); }}>
            <DialogContent
                className="max-w-lg"
                hideClose={isDeploying}
                onInteractOutside={(e) => { if (isDeploying) e.preventDefault(); }}
                onEscapeKeyDown={(e) => { if (isDeploying) e.preventDefault(); }}
            >
                <DialogHeader>
                    <DialogTitle>{deployed ? `${deployed.name} 部署完成` : title}</DialogTitle>
                    {!deployed && (
                        <DialogDescription>
                            左侧是宿主机端口（可改），右侧是容器内端口。默认端口已填好，
                            冲突就改宿主机端口；也可「添加端口」映射自定义服务。WebUI 凭据会自动生成。
                        </DialogDescription>
                    )}
                </DialogHeader>

                {/* 横向留 px + 负 margin 抵消：让 NumberField 聚焦时向外扩的 ring
                    不被 overflow-y-auto 的滚动裁剪边切掉。 */}
                <div className="-mx-1 flex max-h-[60vh] flex-col gap-4 overflow-y-auto px-1 py-1">
                    {deployed ? (
                        // 部署完成：在弹窗内展示结果(WebUI/noVNC 地址 + 凭据),凭据仅一次性
                        // 展示。用户看完点「完成」关闭。
                        <DeployResultBody result={deployed} />
                    ) : isDeploying && progress ? (
                        // 部署进行中：隐藏表单，显示进度区。
                        <div className="flex flex-col gap-3 rounded-md bg-inset/40 px-3 py-3">
                            <p className="text-xs text-text-secondary">{stepHint}</p>
                            <ProgressLine progress={progress} />
                            {recentLogs.length > 0 && (
                                <div className="flex flex-col gap-0.5 rounded-sm bg-canvas/60 px-2 py-1.5">
                                    {recentLogs.map((log, i) => (
                                        <p
                                            key={i}
                                            className="truncate font-mono text-[11px] leading-relaxed text-text-tertiary"
                                        >
                                            {log.message}
                                        </p>
                                    ))}
                                </div>
                            )}
                        </div>
                    ) : (
                        // 未部署时显示表单。
                        <>
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
                        </>
                    )}

                    {error && <p className="text-xs text-danger">{error}</p>}
                </div>

                <DialogFooter>
                    {deployed ? (
                        // 完成态:单个「完成」按钮,点了才关闭(让用户有时间记下凭据)。
                        <Button onClick={onClose}>完成</Button>
                    ) : (
                        <>
                            <Button variant="ghost" onClick={onClose} disabled={isDeploying}>
                                取消
                            </Button>
                            <Button onClick={() => void handleConfirm()} disabled={isDeploying}>
                                {isDeploying && (
                                    <ActionMotionIcon icon={RefreshCw} size={14} motion="spin" />
                                )}
                                开始部署
                            </Button>
                        </>
                    )}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};

// 单行端口映射。
//   preset 行：来自镜像默认，容器端口锁定（显示用途说明），只改宿主机，不可删。
//   自定义行：用户加的，宿主机 + 容器端口都能填，可删。
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

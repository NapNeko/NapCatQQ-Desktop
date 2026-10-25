// 「对接应用端」对话框：选协议 Bot / 应用实例 → 预览 OneBotLinkPlan → 应用。
//
// 应用端页面预填实例、Bot 配置页预填 Bot；两处共用同一份对话框。
// 同机直连；本机 Bot↔远端应用、远端 Bot↔本机应用经桌面 SSH；两台远端走主机常驻隧道。
// Docker Bot 自动配置发布端口或网桥上的专用桥接；具体创建与检查由后端应用计划时执行。

import { useEffect, useMemo, useState } from 'react';
import { Link2 } from 'lucide-react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
    Spinner,
    type SelectItem,
} from '../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION, ExpandPresence } from '../../shared/ui/motion';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { useAppFrameworks, useAppInstances } from '../../hooks/apps/useAppInstances';
import { useAppLinkPlan, useApplyAppLink } from '../../hooks/apps/useAppLink';
import {
    remoteHostIdFromRuntimeTarget,
    isRuntimeTargetLocal,
    runtimeTargetDisplayLabel,
} from '../../core/domain/bot/runtime-target';
import { hostIdDisplayLabel } from './hostLabel';
import { errorText } from '../../core/domain/errors';
import {
    appLinkPairEnabled,
    appLinkPairNote,
    appLinkTransportDescription,
    classifyAppLink,
    isDockerBot,
} from '../../core/domain/apps/appLinkTopology';
import type {
    AppFrameworkManifest,
    AppInstance,
    OneBotLinkMode,
    OneBotLinkPlan,
} from '../../core/ipc/types';

interface AppLinkDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 预填实例（应用端页面入口） */
    instanceId?: string | null;
    /** 预填 Bot（Bot 配置页入口） */
    botId?: string | null;
    /** 应用成功后回调；Bot 配置页用它把连接同步进未保存的表单草稿 */
    onApplied?: (plan: OneBotLinkPlan, instance: AppInstance) => void;
}

function botHostId(runtimeTarget: string): string | null {
    if (isRuntimeTargetLocal(runtimeTarget)) return 'local';
    return remoteHostIdFromRuntimeTarget(runtimeTarget);
}

/** 框架推荐的对接方向；清单还没拉到时按反向（旧三个框架都是反向） */
function linkModeOf(
    frameworks: readonly AppFrameworkManifest[],
    frameworkId: string | undefined,
): OneBotLinkMode {
    return frameworks.find((f) => f.id === frameworkId)?.link_modes[0] ?? 'reverse_ws';
}

export function AppLinkDialog({
    open,
    onOpenChange,
    instanceId: presetInstanceId,
    botId: presetBotId,
    onApplied,
}: AppLinkDialogProps) {
    const { instances } = useAppInstances();
    const { data: frameworks = [] } = useAppFrameworks();
    const { data: snapshots = [] } = useBotSnapshots({ disablePolling: true });
    const configs = useBotConfigsMap(snapshots);
    const { servers } = useServerManager();

    const [instanceId, setInstanceId] = useState<string>(presetInstanceId ?? '');
    const [botId, setBotId] = useState<string>(presetBotId ?? '');

    useEffect(() => {
        if (!open) return;
        setInstanceId(presetInstanceId ?? '');
        setBotId(presetBotId ?? '');
    }, [open, presetInstanceId, presetBotId]);

    const instance = instances.find((i) => i.id === instanceId) ?? null;
    const botConfig = botId ? (configs[botId] ?? null) : null;
    const botHost = botConfig ? botHostId(botConfig.bot.runtime_target) : null;
    const botIsDocker = isDockerBot(botConfig?.bot.deploymentType);
    const { plan, previewing, previewError, retryPreview } = useAppLinkPlan(
        instanceId,
        botId,
        open && !!botConfig && !!instance,
    );
    const applyLink = useApplyAppLink();
    const applying = applyLink.isPending;

    const instanceItems: SelectItem[] = useMemo(
        () =>
            instances.map((i) => {
                const installed = i.state !== 'not_installed' && i.state !== 'installing';
                const allowed = appLinkPairEnabled(botHost, i.host_id);
                const note = !installed
                    ? '（未安装）'
                    : appLinkPairNote(botHost, i.host_id, botConfig?.bot.deploymentType);
                return {
                    value: i.id,
                    label: `${i.display_name} · ${hostIdDisplayLabel(i.host_id, servers)}${note}`,
                    disabled: !installed || !allowed,
                };
            }),
        [instances, botHost, servers, botConfig?.bot.deploymentType],
    );
    const instanceMode = linkModeOf(frameworks, instance?.framework_id);

    const botItems: SelectItem[] = useMemo(
        () =>
            snapshots.map((s) => {
                const cfg = configs[s.bot_id];
                const name = cfg?.bot.name?.trim();
                const host = cfg ? botHostId(cfg.bot.runtime_target) : null;
                const docker = isDockerBot(cfg?.bot.deploymentType);
                const allowed =
                    !instance || !host ? true : appLinkPairEnabled(host, instance.host_id);
                const note =
                    instance && host
                        ? appLinkPairNote(host, instance.host_id, cfg?.bot.deploymentType)
                        : docker
                          ? '（Docker）'
                          : '';
                const where = cfg ? runtimeTargetDisplayLabel(cfg.bot.runtime_target, servers) : '';
                return {
                    value: s.bot_id,
                    label: `${name ? `${name} (${s.bot_id})` : s.bot_id}${where ? ` · ${where}` : ''}${note}`,
                    disabled: !allowed,
                };
            }),
        [snapshots, configs, instance, servers],
    );

    const apply = () => {
        if (!plan || !instanceId || !botId) return;
        applyLink.mutate(
            { instanceId, botId, connectionName: plan.connection.name },
            {
                onSuccess: (next) => {
                    onApplied?.(plan, next);
                    onOpenChange(false);
                },
            },
        );
    };

    const rebinding =
        instance?.link && instance.link.bot_id !== botId ? instance.link.bot_id : null;

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg" dismissOnOutsideClick={!applying}>
                <DialogHeader>
                    <DialogTitle>对接应用端</DialogTitle>
                    <DialogDescription>
                        {instanceMode === 'forward_ws'
                            ? '协议 Bot 开一个 WS 服务，应用端连过来，两端配置一并写入。'
                            : '协议 Bot 反向 WS 连到应用端，两端配置一并写入。'}
                    </DialogDescription>
                </DialogHeader>

                <div className="flex flex-col gap-4">
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                        <Select
                            label="协议 Bot"
                            placeholder="选择 Bot"
                            items={botItems}
                            value={botId}
                            onValueChange={setBotId}
                            disabled={!!presetBotId || applying}
                        />
                        <Select
                            label="应用实例"
                            placeholder="选择实例"
                            items={instanceItems}
                            value={instanceId}
                            onValueChange={setInstanceId}
                            disabled={!!presetInstanceId || applying}
                        />
                    </div>

                    {previewing && (
                        <div className="flex items-center gap-2 text-sm text-text-secondary">
                            <Spinner size="sm" />
                            正在生成对接计划…
                        </div>
                    )}

                    {previewError && (
                        <div
                            role="alert"
                            className="rounded-md border border-danger/20 bg-danger/5 p-3"
                        >
                            <p className="text-xs text-danger">{previewError}</p>
                            <Button
                                variant="ghost"
                                size="sm"
                                className="mt-2"
                                onClick={() => void retryPreview()}
                            >
                                重新生成计划
                            </Button>
                        </div>
                    )}

                    <ExpandPresence visible={!!plan && !previewing}>
                        {plan && (
                            <PlanPreview
                                plan={plan}
                                rebindingFrom={rebinding}
                                topology={classifyAppLink(botHost, instance?.host_id ?? '')}
                                docker={botIsDocker}
                            />
                        )}
                    </ExpandPresence>

                    {applyLink.isError &&
                        applyLink.variables?.botId === botId &&
                        applyLink.variables.instanceId === instanceId && (
                            <p role="alert" className="text-xs text-danger">
                                {errorText(applyLink.error)}
                            </p>
                        )}
                </div>

                <DialogFooter>
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => onOpenChange(false)}
                        disabled={applying}
                    >
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        onClick={apply}
                        disabled={!plan || previewing || applying}
                    >
                        {applying ? (
                            <Spinner size="sm" className="text-white" />
                        ) : (
                            <ActionMotionIcon icon={Link2} size={14} motion={EMPHASIS_MOTION} />
                        )}
                        应用对接
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

function PlanPreview({
    plan,
    rebindingFrom,
    topology,
    docker,
}: {
    plan: OneBotLinkPlan;
    rebindingFrom: string | null;
    topology: ReturnType<typeof classifyAppLink>;
    docker: boolean;
}) {
    const c = plan.connection;
    const transport = appLinkTransportDescription(topology, plan.mode, docker);
    // 跨机时应用端连的是自己机上的隧道口，那个口开隧道时才分，预览里只说经隧道
    const serverLine =
        c.kind === 'ws_server'
            ? docker
                ? `容器内监听 ${c.host}:${c.port}，应用端通过宿主机发布端口连接`
                : topology && topology !== 'same_host'
                  ? `Bot 所在机监听 ${c.host}:${c.port}，应用端经隧道连过来`
                  : `监听 ${c.host}:${c.port}，应用端连过来`
            : null;
    return (
        <div className="flex flex-col gap-3 rounded-md border border-border-subtle bg-inset/40 p-3">
            <div className="border-b border-border-subtle pb-3">
                <p className="mb-1.5 text-2xs uppercase tracking-wide text-text-tertiary">
                    连接方式
                </p>
                <p className="text-xs leading-relaxed text-text-secondary">{transport.route}</p>
                {transport.availability && (
                    <p className="mt-1 text-2xs leading-relaxed text-text-tertiary">
                        {transport.availability}
                    </p>
                )}
                {docker && plan.mode === 'forward_ws' && (
                    <p className="mt-1 text-2xs text-text-tertiary">
                        新增发布端口时会重建运行中的容器，连接短暂中断，数据卷保留。
                    </p>
                )}
                <p className="mt-1 text-2xs text-text-tertiary">
                    应用时创建并检查所需端口与隧道，失败时会显示具体原因。
                </p>
            </div>
            <div>
                <p className="mb-1.5 text-2xs uppercase tracking-wide text-text-tertiary">
                    写入协议 Bot 的连接
                </p>
                <div className="flex flex-wrap items-center gap-2 text-sm">
                    <Badge tone="info" appearance="soft" className="font-mono text-[11px]">
                        {c.kind === 'ws_server' ? 'WS-Server' : 'WS-Client'}
                    </Badge>
                    <span className="font-medium text-text">{c.name}</span>
                    <Badge tone="brand" appearance="soft">
                        应用端
                    </Badge>
                </div>
                <p className="mt-1 break-all font-mono text-2xs text-text-secondary">
                    {serverLine ?? (c.kind === 'ws_client' ? c.url : '')}
                </p>
                <p className="mt-0.5 text-2xs text-text-tertiary">
                    token 已生成；同名连接会被替换。
                </p>
            </div>
            <div>
                <p className="mb-1.5 text-2xs uppercase tracking-wide text-text-tertiary">
                    改动的应用端文件
                </p>
                <ul className="flex flex-col gap-1">
                    {plan.app_side_writes.map((w) => (
                        <li
                            key={w.path}
                            className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-2xs"
                        >
                            <span className="min-w-0 break-all font-mono text-text">{w.path}</span>
                            <span className="min-w-0 break-words text-text-secondary">
                                {w.summary}
                            </span>
                        </li>
                    ))}
                </ul>
            </div>
            {rebindingFrom && (
                <p className="text-2xs text-warning">
                    该实例目前对接的是 Bot {rebindingFrom}，应用后会改绑到 Bot {plan.bot_id}，旧 Bot
                    上的同名连接会被移除。
                </p>
            )}
        </div>
    );
}

// Bot 整套迁移：选目标、预检、执行与回退报告。

import { useEffect, useState } from 'react';
import { ArrowRight, Check, Server, TriangleAlert } from 'lucide-react';
import type { BotMigrationApp } from '../../../core/ipc/generated/domain/BotMigrationApp';
import type { BotMigrationPhase } from '../../../core/ipc/generated/domain/BotMigrationPhase';
import { runtimeTargetDisplayLabel } from '../../../core/domain/bot/runtime-target';
import { useBotMigration } from '../../../hooks/bot/useBotMigration';
import { useServerProfiles } from '../../../hooks/remote/useIsHostReachable';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
    Spinner,
} from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';

const phaseLabels: Record<BotMigrationPhase, string> = {
    preparing: '准备目标环境',
    stopping: '停止源端',
    copying: '复制配置与数据',
    rebuilding: '重建应用依赖',
    switching: '切换运行位置',
    starting: '启动目标端',
    awaiting_login: '等待 QQ 登录',
    completed: '迁移完成',
    failed: '迁移失败',
    rolling_back: '正在回退',
    rolled_back: '已回到源服务器',
    recovery_required: '需要恢复处理',
};
const forwardPhases: BotMigrationPhase[] = [
    'preparing',
    'stopping',
    'copying',
    'rebuilding',
    'switching',
    'starting',
    'awaiting_login',
];

function errorText(error: unknown): string | null {
    if (error == null) return null;
    return error instanceof Error ? error.message : String(error);
}

function MigrationContents({ botId, apps }: { botId: string; apps: BotMigrationApp[] }) {
    return (
        <ul className="divide-y divide-border-subtle rounded-sm border border-border-subtle text-sm">
            <li className="flex items-center justify-between gap-3 px-3 py-2">
                <span>QQ / 协议端</span>
                <span className="font-mono text-xs text-text-secondary">{botId}</span>
            </li>
            {apps.map((app) => (
                <li
                    key={app.instance_id}
                    className="flex items-center justify-between gap-3 px-3 py-2"
                >
                    <span className="min-w-0 truncate" title={app.source_dir}>
                        {app.display_name || app.instance_id}
                    </span>
                    <span className="shrink-0 text-xs text-text-tertiary">{app.framework_id}</span>
                </li>
            ))}
        </ul>
    );
}

interface BotMigrationDialogProps {
    botId: string;
    sourceRuntimeTarget: string;
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onOpenLogin?: () => void;
}

export function BotMigrationDialog({
    botId,
    sourceRuntimeTarget,
    open,
    onOpenChange,
    onOpenLogin,
}: BotMigrationDialogProps) {
    const [targetServerId, setTargetServerId] = useState('');
    const [acknowledged, setAcknowledged] = useState(false);
    const [acceptDataLoss, setAcceptDataLoss] = useState(false);
    const [planningAgain, setPlanningAgain] = useState(false);
    const serversQuery = useServerProfiles(open);
    const servers = serversQuery.data ?? [];
    const migration = useBotMigration(botId, targetServerId, open);
    const { job, running, jobQuery, previewQuery, startMutation, rollbackMutation } = migration;
    const plan = previewQuery.data;
    const canPlanAgain =
        job?.phase === 'completed' ||
        job?.phase === 'rolled_back' ||
        (job?.phase === 'failed' && !job.rollback_available);
    const showPlan = !job || (planningAgain && canPlanAgain);
    const source = sourceRuntimeTarget.replace(/^remote:/, '');
    const targets = servers.filter((server) => server.id !== source);
    const busy = startMutation.isPending || rollbackMutation.isPending;
    const ready =
        migration.available &&
        jobQuery.isSuccess &&
        !jobQuery.isFetching &&
        !!plan &&
        !previewQuery.isFetching &&
        !previewQuery.isError &&
        plan.blockers.length === 0 &&
        !running &&
        !busy &&
        acknowledged;
    const operationError = errorText(startMutation.error ?? rollbackMutation.error);

    useEffect(() => {
        if (open) setAcknowledged(false);
    }, [open, plan?.fingerprint]);
    useEffect(() => {
        setAcceptDataLoss(false);
        if (running) setPlanningAgain(false);
    }, [job?.id, running]);
    useEffect(() => {
        if (open && !targetServerId && targets.length === 1) {
            setTargetServerId(targets[0].id);
        }
    }, [open, targetServerId, targets]);

    const hostLabel = (hostId: string) => runtimeTargetDisplayLabel(hostId, servers);
    const changeTarget = (id: string) => {
        setTargetServerId(id);
        setAcknowledged(false);
        startMutation.reset();
        rollbackMutation.reset();
    };
    const currentStep =
        job?.phase === 'completed'
            ? forwardPhases.length
            : forwardPhases.indexOf(job?.phase ?? 'failed');

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg" className="max-w-xl">
                <DialogHeader>
                    <DialogTitle>迁移到服务器</DialogTitle>
                </DialogHeader>
                <div className="max-h-[65vh] space-y-4 overflow-y-auto px-0.5 py-1">
                    {!migration.available ? (
                        <p
                            role="status"
                            className="rounded-sm bg-inset px-3 py-2 text-sm text-text-secondary"
                        >
                            浏览器预览无法执行服务器迁移，请使用桌面客户端。
                        </p>
                    ) : null}

                    {jobQuery.isPending && migration.available ? (
                        <div className="flex items-center gap-2 py-3 text-sm text-text-secondary">
                            <Spinner size="sm" /> 读取迁移记录
                        </div>
                    ) : null}
                    {jobQuery.error ? (
                        <div role="alert" className="space-y-2 text-sm text-danger">
                            <p>无法读取迁移记录：{errorText(jobQuery.error)}</p>
                            <Button size="sm" onClick={() => void jobQuery.refetch()}>
                                重新读取
                            </Button>
                        </div>
                    ) : null}

                    {showPlan ? (
                        <>
                            <p className="text-sm text-text-secondary">
                                从{' '}
                                <span className="font-medium text-text">
                                    {hostLabel(sourceRuntimeTarget)}
                                </span>{' '}
                                迁出 QQ / 协议端和关联应用。
                            </p>
                            <Select
                                label="目标服务器"
                                placeholder={serversQuery.isPending ? '读取服务器…' : '选择服务器'}
                                value={targetServerId}
                                onValueChange={changeTarget}
                                disabled={
                                    !migration.available ||
                                    running ||
                                    busy ||
                                    serversQuery.isPending
                                }
                                items={targets.map((server) => ({
                                    value: server.id,
                                    label: `${server.name || server.host}${server.state === 'failed' ? '（连接失败）' : ''}`,
                                }))}
                            />
                            {serversQuery.error ? (
                                <div role="alert" className="space-y-2 text-sm text-danger">
                                    <p>无法读取服务器：{errorText(serversQuery.error)}</p>
                                    <Button size="sm" onClick={() => void serversQuery.refetch()}>
                                        重试
                                    </Button>
                                </div>
                            ) : null}
                            {serversQuery.isSuccess && !targets.length ? (
                                <p className="text-sm text-text-secondary">
                                    先在「远程主机」添加一台目标服务器。
                                </p>
                            ) : null}
                            {targetServerId && previewQuery.isFetching ? (
                                <div
                                    role="status"
                                    className="flex items-center gap-2 text-sm text-text-secondary"
                                >
                                    <Spinner size="sm" /> 检查源端、目标端和关联应用
                                </div>
                            ) : null}
                            {previewQuery.error ? (
                                <div role="alert" className="space-y-2 text-sm text-danger">
                                    <p>预检失败：{errorText(previewQuery.error)}</p>
                                    <Button
                                        size="sm"
                                        disabled={previewQuery.isFetching}
                                        onClick={() => void previewQuery.refetch()}
                                    >
                                        重新预检
                                    </Button>
                                </div>
                            ) : null}
                            {plan && !previewQuery.isFetching ? (
                                <>
                                    <MigrationContents botId={botId} apps={plan.apps} />
                                    {plan.blockers.length ? (
                                        <div
                                            role="alert"
                                            className="space-y-2 rounded-sm border border-danger/30 bg-danger/5 p-3 text-sm text-danger"
                                        >
                                            <p className="flex items-center gap-2 font-medium">
                                                <TriangleAlert size={15} /> 解决以下问题后再迁移
                                            </p>
                                            <ul className="list-disc space-y-1 pl-5">
                                                {plan.blockers.map((message, index) => (
                                                    <li key={index}>{message}</li>
                                                ))}
                                            </ul>
                                            <Button
                                                size="sm"
                                                onClick={() => void previewQuery.refetch()}
                                            >
                                                重新预检
                                            </Button>
                                        </div>
                                    ) : (
                                        <div className="space-y-3 text-sm text-text-secondary">
                                            <p>
                                                迁移期间停止源端；保留原目录，目标端重建依赖，QQ
                                                可能需要重新扫码。
                                            </p>
                                            {plan.notes.length ? (
                                                <ul className="list-disc space-y-1 pl-5">
                                                    {plan.notes.map((note, index) => (
                                                        <li key={index}>{note}</li>
                                                    ))}
                                                </ul>
                                            ) : null}
                                            <Checkbox
                                                id={`bot-migration-confirm-${botId}`}
                                                checked={acknowledged}
                                                onCheckedChange={setAcknowledged}
                                                disabled={busy}
                                                label="确认停用源端并开始迁移"
                                            />
                                        </div>
                                    )}
                                </>
                            ) : null}
                        </>
                    ) : job ? (
                        <>
                            <div className="flex flex-wrap items-center gap-2 text-sm text-text-secondary">
                                <Server size={15} />
                                <span>{hostLabel(job.source_host_id)}</span>
                                <ArrowRight size={14} />
                                <span className="font-medium text-text">
                                    {hostLabel(job.target_host_id)}
                                </span>
                            </div>
                            <div className="space-y-2" role="status" aria-live="polite">
                                <p className="flex items-center gap-2 text-sm font-medium">
                                    {running && job.phase !== 'awaiting_login' ? (
                                        <Spinner size="sm" tone="brand" />
                                    ) : null}
                                    {job.phase === 'completed' || job.phase === 'rolled_back' ? (
                                        <Check size={16} className="text-success" />
                                    ) : null}
                                    {phaseLabels[job.phase]}
                                </p>
                                <p className="whitespace-pre-wrap break-words text-sm text-text-secondary">
                                    {job.message}
                                </p>
                            </div>
                            {currentStep >= 0 ? (
                                <ol className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
                                    {forwardPhases.map((phase, index) => (
                                        <li
                                            key={phase}
                                            className={cn(
                                                'flex items-center gap-2',
                                                index === currentStep
                                                    ? 'font-medium text-brand'
                                                    : index < currentStep
                                                      ? 'text-text-secondary'
                                                      : 'text-text-disabled',
                                            )}
                                        >
                                            {index < currentStep ? (
                                                <Check size={13} />
                                            ) : (
                                                <span className="w-[13px] text-center tabular-nums">
                                                    {index + 1}
                                                </span>
                                            )}
                                            {phaseLabels[phase]}
                                        </li>
                                    ))}
                                </ol>
                            ) : null}
                            {job.phase === 'awaiting_login' ? (
                                <div className="space-y-2 rounded-sm bg-brand/5 px-3 py-2 text-sm text-text-secondary">
                                    <p>目标端已启动，检测到 QQ 在线和应用就绪后会自动完成。</p>
                                    {onOpenLogin ? (
                                        <Button
                                            size="sm"
                                            variant="primary"
                                            onClick={() => {
                                                onOpenChange(false);
                                                onOpenLogin();
                                            }}
                                        >
                                            登录 QQ
                                        </Button>
                                    ) : (
                                        <p>需要扫码时，可从 Bot 卡片打开登录窗口。</p>
                                    )}
                                </div>
                            ) : null}
                            <MigrationContents botId={botId} apps={job.apps} />
                            {job.error ? (
                                <p
                                    role="alert"
                                    className="whitespace-pre-wrap break-words rounded-sm bg-danger/5 px-3 py-2 text-sm text-danger"
                                >
                                    {job.error}
                                </p>
                            ) : null}
                            {job.rollback_available &&
                            (!running || job.phase === 'awaiting_login') ? (
                                <div className="space-y-3 border-t border-border-subtle pt-3 text-sm text-text-secondary">
                                    <p>
                                        回退会停止当前端，恢复源端配置和原先的启停状态。两边目录保留。
                                    </p>
                                    {job.target_started ? (
                                        <Checkbox
                                            id={`bot-migration-rollback-${botId}`}
                                            checked={acceptDataLoss}
                                            onCheckedChange={setAcceptDataLoss}
                                            disabled={busy}
                                            label="目标端产生的新数据不会自动回拷；我确认回退"
                                        />
                                    ) : null}
                                    <Button
                                        variant="secondary"
                                        disabled={busy || (job.target_started && !acceptDataLoss)}
                                        onClick={() => rollbackMutation.mutate(acceptDataLoss)}
                                    >
                                        {rollbackMutation.isPending ? <Spinner size="sm" /> : null}{' '}
                                        回到源服务器
                                    </Button>
                                </div>
                            ) : null}
                            {running ? (
                                <p className="text-xs text-text-tertiary">
                                    可以关闭窗口，迁移会在后台继续。再次从 Bot 菜单进入可查看进度。
                                </p>
                            ) : null}
                        </>
                    ) : null}
                    {planningAgain && job ? (
                        <details className="text-xs text-text-secondary">
                            <summary className="cursor-pointer">
                                上次迁移记录 · {phaseLabels[job.phase]}
                            </summary>
                            <p className="mt-2 whitespace-pre-wrap break-words">{job.message}</p>
                            {job.error ? (
                                <p className="mt-1 whitespace-pre-wrap break-words text-danger">
                                    {job.error}
                                </p>
                            ) : null}
                        </details>
                    ) : null}
                    {operationError ? (
                        <p
                            role="alert"
                            className="whitespace-pre-wrap break-words text-sm text-danger"
                        >
                            {operationError}
                        </p>
                    ) : null}
                </div>
                <DialogFooter className="gap-2">
                    <Button onClick={() => onOpenChange(false)}>
                        {running ? '后台继续' : '关闭'}
                    </Button>
                    {!showPlan && canPlanAgain ? (
                        <Button
                            onClick={() => {
                                changeTarget('');
                                setPlanningAgain(true);
                            }}
                        >
                            再次迁移
                        </Button>
                    ) : null}
                    {showPlan ? (
                        <Button
                            variant="primary"
                            disabled={!ready}
                            onClick={() => startMutation.mutate()}
                        >
                            {startMutation.isPending ? <Spinner size="sm" /> : null} 开始迁移
                        </Button>
                    ) : null}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

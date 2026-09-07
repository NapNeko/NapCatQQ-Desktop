// 「对接应用端」对话框：选协议 Bot / 应用实例 → 预览 OneBotLinkPlan → 应用。
//
// 应用端页面预填实例、Bot 配置页预填 Bot；两处共用同一份对话框。
// 首发只允许同机对接（Bot 的 runtime_target 与实例 host_id 同机），不同机的选项置灰。

import { useEffect, useMemo, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
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
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../shared/ui/motion';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { useAppInstances, invalidateBotConfigAfterLink } from '../../hooks/apps/useAppInstances';
import { appConfigKey } from '../../hooks/apps/useAppInstanceConfig';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import {
    remoteHostIdFromRuntimeTarget,
    isRuntimeTargetLocal,
    runtimeTargetDisplayLabel,
} from '../../core/domain/bot/runtime-target';
import { hostIdDisplayLabel } from './hostLabel';
import type { AppInstance, OneBotLinkPlan } from '../../core/ipc/types';

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

export function AppLinkDialog({
    open,
    onOpenChange,
    instanceId: presetInstanceId,
    botId: presetBotId,
    onApplied,
}: AppLinkDialogProps) {
    const queryClient = useQueryClient();
    const { instances, patch } = useAppInstances();
    const { data: snapshots = [] } = useBotSnapshots({ disablePolling: true });
    const configs = useBotConfigsMap(snapshots);
    const { servers } = useServerManager();

    const [instanceId, setInstanceId] = useState<string>(presetInstanceId ?? '');
    const [botId, setBotId] = useState<string>(presetBotId ?? '');
    const [plan, setPlan] = useState<OneBotLinkPlan | null>(null);
    const [previewError, setPreviewError] = useState<string | null>(null);
    const [previewing, setPreviewing] = useState(false);
    const [applying, setApplying] = useState(false);

    useEffect(() => {
        if (!open) return;
        setInstanceId(presetInstanceId ?? '');
        setBotId(presetBotId ?? '');
        setPlan(null);
        setPreviewError(null);
    }, [open, presetInstanceId, presetBotId]);

    const instance = instances.find((i) => i.id === instanceId) ?? null;
    const botConfig = botId ? configs[botId] ?? null : null;
    const botHost = botConfig ? botHostId(botConfig.bot.runtime_target) : null;

    const instanceItems: SelectItem[] = useMemo(
        () =>
            instances.map((i) => {
                const installed = i.state !== 'not_installed' && i.state !== 'installing';
                const sameHost = botHost ? i.host_id === botHost : true;
                return {
                    value: i.id,
                    label: `${i.display_name} · ${hostIdDisplayLabel(i.host_id, servers)}${
                        !installed ? '（未安装）' : !sameHost ? '（不同机）' : ''
                    }`,
                    disabled: !installed || !sameHost,
                };
            }),
        [instances, botHost, servers],
    );

    const botItems: SelectItem[] = useMemo(
        () =>
            snapshots.map((s) => {
                const cfg = configs[s.bot_id];
                const name = cfg?.bot.name?.trim();
                const host = cfg ? botHostId(cfg.bot.runtime_target) : null;
                const sameHost = instance && host ? host === instance.host_id : true;
                const where = cfg ? runtimeTargetDisplayLabel(cfg.bot.runtime_target, servers) : '';
                return {
                    value: s.bot_id,
                    label: `${name ? `${name} (${s.bot_id})` : s.bot_id}${where ? ` · ${where}` : ''}${
                        !sameHost ? '（不同机）' : ''
                    }`,
                    disabled: !sameHost,
                };
            }),
        [snapshots, configs, instance, servers],
    );

    useEffect(() => {
        if (!open || !instanceId || !botId) {
            setPlan(null);
            return;
        }
        let cancelled = false;
        setPreviewing(true);
        setPreviewError(null);
        appFrameworkService
            .previewLink(instanceId, botId)
            .then((p) => {
                if (!cancelled) setPlan(p);
            })
            .catch((err) => {
                if (!cancelled) {
                    setPlan(null);
                    setPreviewError(errorText(err));
                }
            })
            .finally(() => {
                if (!cancelled) setPreviewing(false);
            });
        return () => {
            cancelled = true;
        };
    }, [open, instanceId, botId]);

    const apply = async () => {
        if (!plan || !instanceId || !botId) return;
        setApplying(true);
        try {
            const next = await appFrameworkService.applyLink(instanceId, botId);
            patch(next);
            invalidateBotConfigAfterLink(queryClient, botId);
            queryClient.invalidateQueries({ queryKey: appConfigKey(instanceId) });
            queryClient.invalidateQueries({ queryKey: ['appConfigText', instanceId] });
            pushInfoBar({
                key: `app-link:${next.id}`,
                tone: 'success',
                title: '对接完成',
                content: `Bot ${botId} 已加入连接 ${plan.connection.name}，运行中的 Bot 会热更新。`,
                autoDismissMs: 4000,
            });
            onApplied?.(plan, next);
            onOpenChange(false);
        } catch (err) {
            pushInfoBar({
                key: `app-link:${instanceId}`,
                tone: 'danger',
                title: '对接失败',
                content: errorText(err),
            });
        } finally {
            setApplying(false);
        }
    };

    const rebinding = instance?.link && instance.link.bot_id !== botId ? instance.link.bot_id : null;

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg" dismissOnOutsideClick={!applying}>
                <DialogHeader>
                    <DialogTitle>对接应用端</DialogTitle>
                    <DialogDescription>协议 Bot 以反向 WS 连到应用端，双方配置一并写入。</DialogDescription>
                </DialogHeader>

                <div className="flex flex-col gap-4">
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                        <Select
                            label="协议 Bot"
                            placeholder="选择 Bot"
                            items={botItems}
                            value={botId || undefined}
                            onValueChange={setBotId}
                            disabled={!!presetBotId}
                        />
                        <Select
                            label="应用实例"
                            placeholder="选择实例"
                            items={instanceItems}
                            value={instanceId || undefined}
                            onValueChange={setInstanceId}
                            disabled={!!presetInstanceId}
                        />
                    </div>

                    {previewing && (
                        <div className="flex items-center gap-2 text-sm text-text-secondary">
                            <Spinner size="sm" />
                            正在生成对接计划…
                        </div>
                    )}

                    {previewError && (
                        <p className="rounded-sm border border-danger/40 bg-danger-soft px-3 py-2 text-sm text-danger">
                            {previewError}
                        </p>
                    )}

                    {plan && !previewing && (
                        <PlanPreview plan={plan} rebindingFrom={rebinding} />
                    )}
                </div>

                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)} disabled={applying}>
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

function PlanPreview({ plan, rebindingFrom }: { plan: OneBotLinkPlan; rebindingFrom: string | null }) {
    const c = plan.connection;
    return (
        <div className="flex flex-col gap-3 rounded-md border border-border-subtle bg-inset/40 p-3">
            <div>
                <p className="mb-1.5 text-2xs uppercase tracking-wide text-text-tertiary">
                    写入协议 Bot 的连接
                </p>
                <div className="flex flex-wrap items-center gap-2 text-sm">
                    <Badge tone="info" appearance="soft" className="font-mono text-[11px]">
                        WS-Client
                    </Badge>
                    <span className="font-medium text-text">{c.name}</span>
                    <Badge tone="brand" appearance="soft">
                        应用端
                    </Badge>
                </div>
                <p className="mt-1 break-all font-mono text-2xs text-text-secondary">{c.url}</p>
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
                        <li key={w.path} className="flex items-baseline gap-2 text-2xs">
                            <span className="shrink-0 font-mono text-text">{w.path}</span>
                            <span className="text-text-secondary">{w.summary}</span>
                        </li>
                    ))}
                </ul>
            </div>
            {rebindingFrom && (
                <p className="text-2xs text-warning">
                    该实例目前对接的是 Bot {rebindingFrom}，应用后会改绑到 Bot {plan.bot_id}，旧 Bot 上的同名连接会被移除。
                </p>
            )}
        </div>
    );
}

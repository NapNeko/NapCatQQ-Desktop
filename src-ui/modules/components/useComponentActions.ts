// 组件页组件操作编排：启停/更新/卸载任务发起、生命周期门禁、SnowLuma 选包。
// 从 ComponentsPage.next 外提；Docker 安装与 sudo 弹框在 useDockerSudoOps
// （两边共用 startAction/onTaskTerminal/refetch，startQqDepsRepair 在那边）。

import { useCallback, useState } from 'react';
import type { QueryClient } from '@tanstack/react-query';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';
import {
    componentMutationBlockedReason,
    componentLifecycleBlockedReason,
} from '../../core/domain/components/mutation-gate';
import type { UseComponentActionResult } from '../../hooks/components/useComponentAction';
import type {
    BotActorSnapshot,
    ComponentId,
    SnowLumaPackage,
    StepKind,
} from '../../core/ipc/types';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';

// SnowLuma 在 Linux 装前要选发行版包，弹框确认后的载荷。
export interface SnowLumaPackagePrompt {
    componentId: ComponentId;
    hostId: string;
    stepKind: StepKind;
}

export interface UseComponentActionsInput {
    hostNameOf: (hostId: string) => string;
    refetch: () => void;
    queryClient: QueryClient;
    action: UseComponentActionResult;
    botSnapshots: readonly BotActorSnapshot[];
    botConfigs: Readonly<Record<string, BotConfig | null>>;
    probeQqDependencies: (hostId: string, force?: boolean) => Promise<void>;
}

export interface UseComponentActionsResult {
    slPkgPrompt: SnowLumaPackagePrompt | null;
    closeSlPkgPrompt: () => void;
    confirmSnowLumaPackage: (pkg: SnowLumaPackage) => void;
    handleAction: (
        componentId: ComponentId,
        hostId: string,
        payload: { stepKind: StepKind } | { cancelTaskId: string },
    ) => Promise<void>;
    lifecycleBlockedReasonForHost: (hostId: string) => string | null;
}

export function useComponentActions(args: UseComponentActionsInput): UseComponentActionsResult {
    const {
        hostNameOf,
        refetch,
        queryClient,
        action: { startAction, cancelAction, onTaskTerminal },
        botSnapshots,
        botConfigs,
        probeQqDependencies,
    } = args;

    const [slPkgPrompt, setSlPkgPrompt] = useState<SnowLumaPackagePrompt | null>(null);

    const beginComponentAction = useCallback(
        async (
            componentId: ComponentId,
            hostId: string,
            stepKind: StepKind,
            options?: { snowlumaLinuxPackage?: SnowLumaPackage },
        ) => {
            const taskId = await startAction(componentId, hostId, stepKind, options);
            onTaskTerminal(taskId, (status) => {
                // 只在成功时刷新状态；失败/取消时不刷新，避免部分删除导致探测返回 None 误显示"未安装"。
                if (status === 'success') {
                    refetch();
                    if (componentId === 'snowluma') {
                        void queryClient.invalidateQueries({ queryKey: ['appSettings'] });
                    }
                    if (componentId === 'qq') {
                        void probeQqDependencies(hostId, true);
                    }
                }
            });
        },
        [startAction, onTaskTerminal, refetch, probeQqDependencies, queryClient],
    );

    const reportActionStartError = useCallback(
        (componentId: ComponentId, hostId: string, err: unknown) => {
            pushErrorBar({
                key: `component-action-start:${componentId}:${hostId}`,
                title: `组件操作失败 · ${hostNameOf(hostId)}`,
                raw: errorText(err, '组件操作失败，请稍后重试'),
            });
        },
        [hostNameOf],
    );

    const handleAction = useCallback(
        async (
            componentId: ComponentId,
            hostId: string,
            payload: { stepKind: StepKind } | { cancelTaskId: string },
        ) => {
            try {
                if ('cancelTaskId' in payload) {
                    await cancelAction(payload.cancelTaskId);
                    return;
                }
                const blocked = componentMutationBlockedReason(
                    botSnapshots,
                    botConfigs,
                    hostId,
                    payload.stepKind,
                );
                if (blocked) {
                    globalInfoBarStore.push({
                        key: `component-action-blocked:${componentId}:${hostId}:${payload.stepKind}`,
                        tone: 'warning',
                        title: `无法${payload.stepKind === 'update' ? '更新' : '卸载'} · ${hostNameOf(hostId)}`,
                        content: blocked,
                        autoDismissMs: 8_000,
                    });
                    return;
                }
                if (
                    componentId === 'snowluma' &&
                    (payload.stepKind === 'ensure_installed' ||
                        payload.stepKind === 'force_install')
                ) {
                    setSlPkgPrompt({
                        componentId,
                        hostId,
                        stepKind: payload.stepKind,
                    });
                    return;
                }
                await beginComponentAction(componentId, hostId, payload.stepKind);
            } catch (err) {
                reportActionStartError(componentId, hostId, err);
            }
        },
        [
            beginComponentAction,
            cancelAction,
            hostNameOf,
            reportActionStartError,
            botSnapshots,
            botConfigs,
        ],
    );

    const confirmSnowLumaPackage = useCallback(
        (pkg: SnowLumaPackage) => {
            const pending = slPkgPrompt;
            if (!pending) return;
            setSlPkgPrompt(null);

            void beginComponentAction(pending.componentId, pending.hostId, pending.stepKind, {
                snowlumaLinuxPackage: pkg,
            }).catch((err) => {
                reportActionStartError(pending.componentId, pending.hostId, err);
            });
        },
        [slPkgPrompt, beginComponentAction, reportActionStartError],
    );

    const lifecycleBlockedReasonForHost = useCallback(
        (hostId: string) => componentLifecycleBlockedReason(botSnapshots, botConfigs, hostId),
        [botSnapshots, botConfigs],
    );

    return {
        slPkgPrompt,
        closeSlPkgPrompt: () => setSlPkgPrompt(null),
        confirmSnowLumaPackage,
        handleAction,
        lifecycleBlockedReasonForHost,
    };
}

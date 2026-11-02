// 卡片的业务适配层：每种运行时能力只解析一次，操作入口共用决策及执行状态。
import { useEffect, useRef, useState } from 'react';
import {
    buildBotCardActions,
    type BotCardActionId,
} from '../../../../core/domain/bot/card-actions';
import { isBotActive, isBotStarting } from '../../../../core/domain/bot/status';
import { isWebuiAvailable, webuiTooltip } from '../../../../core/domain/webui/availability';
import {
    isSnowlumaRemoteNativeConfig,
    isSnowlumaRemoteUiRetryAvailable,
    isSnowlumaTunnelReady,
} from '../../../../core/domain/bot/snowluma-remote-ui';
import {
    isRuntimeTargetLocal,
    remoteHostIdFromRuntimeTarget,
    runtimeTargetDisplayLabel,
    runtimeTargetTooltip,
} from '../../../../core/domain/bot/runtime-target';
import { localOs } from '../../../../core/domain/runtime/env';
import {
    useServerProfiles,
    isHostReachableFromCache,
} from '../../../../hooks/remote/useIsHostReachable';
import { useFeatureEnabled } from '../../../../hooks/preferences/featureTogglesStore';
import { useDebugConsoleEnabled } from '../../../../hooks/debug/useDebugConsoleEnabled';
import { useRemoteSnowlumaQrEnabled } from '../../../../hooks/webui/useRemoteSnowlumaQrLogin';
import { openTerminal } from '../../../../hooks/terminal/terminalStore';
import { openDebugConsole } from '../../../../hooks/debug/debugNav';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { buildBotListCardStatus } from './botCardPresentation';
import type { BotCardProps } from './BotCard.types';

export function useBotCardController(props: BotCardProps) {
    const { bot, config, flavor } = props;
    const botId = bot.bot_id;
    const [qrOpen, setQrOpen] = useState(false);
    const [remoteQrOpen, setRemoteQrOpen] = useState(false);
    const [migrationOpen, setMigrationOpen] = useState(false);
    const [retryingUi, setRetryingUi] = useState(false);
    const retryInFlight = useRef(false);
    const terminalEnabled = useFeatureEnabled('terminal');
    const debugEnabled = useDebugConsoleEnabled();
    const runtimeTarget = config?.bot.runtime_target ?? 'local';
    const isRemote = !isRuntimeTargetLocal(runtimeTarget);
    const remoteHostId = remoteHostIdFromRuntimeTarget(runtimeTarget);
    const { data: servers = [] } = useServerProfiles(isRemote);
    const transportFailed =
        remoteHostId != null && !isHostReachableFromCache(remoteHostId, servers);
    const localQrEligible =
        localOs() === 'windows' &&
        config?.bot.backend_type === 'snowluma' &&
        !isRemote &&
        config.bot.deploymentType === 'native' &&
        (config.bot.snowlumaStartMode?.mode ?? 'cold_start') === 'cold_start';
    const remoteQrEnabled = useRemoteSnowlumaQrEnabled(
        localQrEligible ||
            (config?.bot.backend_type === 'snowluma' &&
                (config.bot.deploymentType === 'docker' || isRemote)),
        localQrEligible,
    );
    const remoteQrAvailable =
        remoteQrEnabled && bot.state === 'running' && props.snowlumaLoginState !== 'logged_in';
    useEffect(() => {
        if (!remoteQrAvailable) setRemoteQrOpen(false);
    }, [remoteQrAvailable]);

    const active = isBotActive(bot.state);
    const webui = isWebuiAvailable({
        flavor,
        napcat: props.napcatBinding ?? null,
        snowlumaDaemonState: props.snowlumaDaemonState ?? null,
        snowlumaDockerEndpointsReady: props.snowlumaDockerEndpointsReady ?? false,
        snowlumaRemoteNativeTunnelReady:
            isSnowlumaRemoteNativeConfig(config ?? null) && props.snowlumaDaemonState === 'ready',
    });
    const novnc =
        !!props.onOpenNovnc &&
        !!props.isSnowlumaRemoteTunnelUi &&
        active &&
        isSnowlumaTunnelReady({
            config: config ?? null,
            dockerEndpointsReady: props.snowlumaDockerEndpointsReady ?? false,
            daemonState: props.snowlumaDaemonState ?? null,
        });
    const actions = buildBotCardActions({
        state: bot.state,
        actionPending: props.actionPending ?? false,
        transportFailed,
        isDocker: config?.bot.deploymentType === 'docker',
        retryingUi,
        capabilities: {
            webui,
            webuiTooltip: webuiTooltip({ flavor, available: webui }),
            novnc,
            qr: !!props.qrcodeUrl,
            remoteQr: remoteQrAvailable,
            retryUi:
                !!props.onRetrySnowlumaUi &&
                isSnowlumaRemoteUiRetryAvailable({
                    config: config ?? null,
                    active: bot.state === 'running',
                    transportFailed,
                }),
            terminal: terminalEnabled,
            debug: debugEnabled,
        },
    });

    const retryUi = async () => {
        // ref 在同一事件周期内生效；工具栏与右键菜单共享一次重建请求。
        if (retryInFlight.current || !props.onRetrySnowlumaUi) return;
        retryInFlight.current = true;
        setRetryingUi(true);
        try {
            await props.onRetrySnowlumaUi(botId);
        } finally {
            retryInFlight.current = false;
            setRetryingUi(false);
        }
    };
    const copyQqid = async () => {
        try {
            await navigator.clipboard.writeText(botId);
            pushInfoBar({
                tone: 'info',
                title: '已复制 QQ 号',
                content: botId,
                autoDismissMs: 2000,
            });
        } catch {
            /* 剪贴板不可用时不打断卡片操作。 */
        }
    };
    const onAction = (id: BotCardActionId) => {
        const action = actions.find((item) => item.id === id);
        if (!action || action.disabled) return;
        switch (id) {
            case 'start':
                props.onStart(botId);
                break;
            case 'stop':
                props.onStop(botId);
                break;
            case 'configure':
                props.onConfigure(botId);
                break;
            case 'logs':
                props.onViewLogs(botId);
                break;
            case 'metrics':
                props.onViewMetrics(botId);
                break;
            case 'webui':
                props.onOpenWebui({ botId, flavor, napcat: props.napcatBinding ?? null });
                break;
            case 'novnc':
                props.onOpenNovnc?.(botId);
                break;
            case 'qr':
                setQrOpen(true);
                break;
            case 'remoteQr':
                setRemoteQrOpen(true);
                break;
            case 'migrate':
                setMigrationOpen(true);
                break;
            case 'retryUi':
                void retryUi();
                break;
            case 'debug':
                openDebugConsole(botId);
                break;
            case 'terminal':
            case 'hostTerminal':
                void openTerminal({ kind: 'bot', bot_id: botId, host_dir: id === 'hostTerminal' });
                break;
            case 'copy':
                void copyQqid();
                break;
        }
    };
    return {
        displayName: config?.bot.name.trim() || botId,
        runtime: {
            remote: isRemote,
            transportFailed,
            label: runtimeTargetDisplayLabel(runtimeTarget, servers),
            tooltip: runtimeTargetTooltip(runtimeTarget, servers),
        },
        status: buildBotListCardStatus({
            state: bot.state,
            flavor,
            pendingRestart: !!bot.pending_restart,
            needsQrLogin: !!props.qrcodeUrl && props.isOnline !== true,
            isOnline: props.isOnline,
            napcatAuthIssue: props.napcatAuthIssue,
            snowlumaLoginState: props.snowlumaLoginState,
            snowlumaDaemonState: props.snowlumaDaemonState,
            snowlumaProbeUnavailable: props.snowlumaProbeUnavailable,
            snowlumaDockerEndpointsReady: props.snowlumaDockerEndpointsReady ?? false,
        }),
        accent: transportFailed
            ? ('danger' as const)
            : isBotStarting(bot.state) || bot.state === 'repairing'
              ? ('brand' as const)
              : ('none' as const),
        actions,
        onAction,
        dialogs: {
            botId,
            runtimeTarget,
            qrOpen,
            setQrOpen,
            remoteQrOpen,
            setRemoteQrOpen,
            migrationOpen,
            setMigrationOpen,
            remoteQrEnabled,
            qrcodeUrl: props.qrcodeUrl ?? null,
            isOnline: props.isOnline,
            invalidationReason: props.invalidationReason,
            remoteOnline: props.snowlumaLoginState === 'logged_in',
            running: bot.state === 'running',
            onOpenVnc: novnc ? () => onAction('novnc') : undefined,
            onOpenLogin: props.qrcodeUrl
                ? () => onAction('qr')
                : remoteQrAvailable
                  ? () => onAction('remoteQr')
                  : novnc
                    ? () => onAction('novnc')
                    : undefined,
        },
    };
}
export type BotCardDialogsModel = ReturnType<typeof useBotCardController>['dialogs'];

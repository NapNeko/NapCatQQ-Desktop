import type {
    BotActorSnapshot,
    DaemonState,
    NapCatLoginInvalidationReason,
    NapCatWebuiAuthIssue,
    SnowLumaLoginState,
} from '../../../../core/ipc/types';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import type { Flavor } from '../../../../core/domain/bot/flavor';
import type { NapcatWebuiBinding } from '../../../../core/domain/webui/availability';

export interface BotCardProps {
    bot: BotActorSnapshot;
    config?: BotConfig | null;
    flavor: Flavor | null;
    qrcodeUrl?: string | null;
    isOnline?: boolean | null;
    invalidationReason?: NapCatLoginInvalidationReason | null;
    napcatAuthIssue?: NapCatWebuiAuthIssue | null;
    napcatBinding?: NapcatWebuiBinding | null;
    /** 仅用于 WebUI 可用性判断，不在卡片上展示 */
    snowlumaDaemonState?: DaemonState | null;
    /** 远端 SnowLuma Docker：隧道就绪（Native 远端用 daemon + 同套 IPC） */
    snowlumaDockerEndpointsReady?: boolean;
    snowlumaLoginState?: SnowLumaLoginState | null;
    snowlumaProbeUnavailable?: boolean;

    isBatchMode: boolean;
    isSelected: boolean;
    actionPending?: boolean;

    onStart: (botId: string) => void;
    onStop: (botId: string) => void;
    onConfigure: (botId: string) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onToggleSelect: (botId: string) => void;
    onOpenWebui: (params: {
        botId: string;
        flavor: Flavor | null;
        napcat: NapcatWebuiBinding | null;
    }) => void;
    /** 远端 SnowLuma：Docker 或 Native 下展示 noVNC（扫码） */
    isSnowlumaRemoteTunnelUi?: boolean;
    onOpenNovnc?: (botId: string) => void;
    /** 远端 SnowLuma 隧道失效时重建 WebUI/noVNC，不重启 Bot。 */
    onRetrySnowlumaUi?: (botId: string) => Promise<void>;
}

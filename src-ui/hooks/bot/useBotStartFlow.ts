// Bot 启动编排：Desktop 协议 → 运行时 / Docker 门禁 → 系统 QQ 提醒 → 配置漂移 →
// SnowLuma 协议 → start。编排链路要直连 botService / componentService，modules 层
// 禁止 services，且这条链挂着一串互锁状态（漂移、协议、启动中、被抑制的错误），
// 拆开散回页面会打架，整体收在 hook 里，页面只做装配与渲染。

import { useCallback, useEffect, useRef, useState } from 'react';
import { botService } from '../../core/services/bot.service';
import { componentService } from '../../core/services/component.service';
import { errorText } from '../../core/domain/errors';
import { isDesktopConsentError, isSnowLumaConsentError } from '../../core/domain/bot/consent-error';
import {
    dismissSystemQqWarning,
    isSystemQqWarningDismissed,
    launchesLocalQq,
} from '../../core/domain/bot/system-qq-warning';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { requestDesktopConsent } from '../desktop/desktopConsentHost';
import type { BotActorSnapshot } from '../../core/ipc/types';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import type { SnowLumaAgreementsPayload } from '../../core/ipc/generated/SnowLumaAgreementsPayload';
import type { ConfigDrift } from '../../core/ipc/generated/ConfigDrift';
import type { DriftDecision } from '../../core/ipc/generated/DriftDecision';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { useBotMutations } from './useBotMutations';

export function useBotStartFlow({
    botSnapshots,
    configByBot,
    dockerStartGate,
    runtimeStartGate,
    mutations,
    onNavigate,
}: {
    botSnapshots: BotActorSnapshot[];
    configByBot: Record<string, BotConfig | null>;
    dockerStartGate: (botId: string) => string | null;
    runtimeStartGate: (botId: string) => string | null;
    mutations: ReturnType<typeof useBotMutations>;
    onNavigate?: (route: AppRoute) => void;
}) {
    // ── Config drift detection before start ──────────────────────────────
    const [pendingDrift, setPendingDrift] = useState<ConfigDrift | null>(null);
    const [driftBotId, setDriftBotId] = useState<string | null>(null);
    const [consentBotId, setConsentBotId] = useState<string | null>(null);
    const [consentPayload, setConsentPayload] = useState<SnowLumaAgreementsPayload | null>(null);
    const [consentSubmitting, setConsentSubmitting] = useState(false);
    const [consentRetryDecisions, setConsentRetryDecisions] = useState<DriftDecision[] | null>(
        null,
    );
    const [startingBotId, setStartingBotId] = useState<string | null>(null);
    const [systemQqBotId, setSystemQqBotId] = useState<string | null>(null);
    const activeConsentBotRef = useRef<string | null>(null);
    const openingConsentBotRef = useRef<string | null>(null);
    const suppressedConsentErrorsRef = useRef<Map<string, string>>(new Map());

    useEffect(() => {
        activeConsentBotRef.current = consentBotId;
    }, [consentBotId]);

    const suppressCurrentConsentError = useCallback(
        (botId: string) => {
            const currentError = botSnapshots
                .find((bot) => bot.bot_id === botId)
                ?.last_error?.trim();
            if (currentError && isSnowLumaConsentError(currentError)) {
                suppressedConsentErrorsRef.current.set(botId, currentError);
            }
        },
        [botSnapshots],
    );

    const clearConsentErrorSuppression = useCallback((botId: string) => {
        suppressedConsentErrorsRef.current.delete(botId);
    }, []);

    const openSnowLumaConsent = useCallback(
        async (
            botId: string,
            decisions: DriftDecision[] | null = null,
            preparedPayload?: SnowLumaAgreementsPayload,
        ) => {
            const activeBot = activeConsentBotRef.current;
            if (activeBot || openingConsentBotRef.current) return;
            activeConsentBotRef.current = botId;
            openingConsentBotRef.current = botId;
            setConsentBotId(botId);
            setConsentRetryDecisions(decisions);
            if (preparedPayload) {
                setConsentPayload(preparedPayload);
                openingConsentBotRef.current = null;
                return;
            }
            setConsentPayload(null);
            try {
                const payload = await botService.getSnowLumaAgreements(botId);
                setConsentPayload(payload);
            } catch (err: unknown) {
                if (activeConsentBotRef.current === botId) activeConsentBotRef.current = null;
                setConsentBotId(null);
                setConsentRetryDecisions(null);
                pushErrorBar({
                    title: '读取 SnowLuma 协议失败',
                    raw: errorText(err),
                });
            } finally {
                if (openingConsentBotRef.current === botId) openingConsentBotRef.current = null;
            }
        },
        [],
    );

    const prepareSnowLumaConsentOrOpen = useCallback(
        async (botId: string, decisions: DriftDecision[] | null = null) => {
            try {
                const payload = await botService.prepareSnowLumaAgreements(botId);
                if (payload?.consent_required) {
                    openSnowLumaConsent(botId, decisions, payload);
                    return false;
                }
                return true;
            } catch (err: unknown) {
                if (isSnowLumaConsentError(err)) {
                    await openSnowLumaConsent(botId, decisions);
                    return false;
                }
                pushErrorBar({
                    title: 'SnowLuma 启动前检查失败',
                    raw: errorText(err),
                });
                return false;
            }
        },
        [openSnowLumaConsent],
    );

    const startBotDirect = useCallback(
        async (botId: string) => {
            setStartingBotId(botId);
            const ready = await prepareSnowLumaConsentOrOpen(botId);
            if (!ready) {
                setStartingBotId(null);
                return;
            }
            try {
                await mutations.startBotAsync(botId);
            } catch (err: unknown) {
                if (isDesktopConsentError(err)) {
                    await requestDesktopConsent(async () => {
                        const again = await prepareSnowLumaConsentOrOpen(botId);
                        if (!again) return;
                        await mutations.startBotAsync(botId);
                    });
                    return;
                }
                if (isSnowLumaConsentError(err)) {
                    await openSnowLumaConsent(botId);
                    return;
                }
                throw err;
            } finally {
                setStartingBotId(null);
            }
        },
        [mutations, openSnowLumaConsent, prepareSnowLumaConsentOrOpen],
    );

    // 配置漂移 → SnowLuma 协议 → start；系统 QQ 提醒点「继续启动」后也从这里接着走
    const startAfterQqCheck = useCallback(
        async (botId: string) => {
            setStartingBotId(botId);
            let drift: ConfigDrift | null;
            try {
                drift = await botService.detectConfigDrift(botId);
            } catch {
                drift = null;
            }
            if (drift && (drift.added.length > 0 || drift.modified.length > 0)) {
                setDriftBotId(botId);
                setPendingDrift(drift);
                setStartingBotId(null);
                return;
            }
            startBotDirect(botId).catch(() => undefined);
        },
        [startBotDirect],
    );

    // 探测失败不拦启动，提醒而已
    const needsSystemQqWarning = useCallback(
        async (botId: string) => {
            const config = configByBot[botId];
            if (!config || !launchesLocalQq(config) || isSystemQqWarningDismissed()) return false;
            try {
                return (await componentService.localQqSource()) === 'system';
            } catch {
                return false;
            }
        },
        [configByBot],
    );

    const handleStartBot = useCallback(
        async (botId: string) => {
            // 顺序：Desktop 协议 → 运行时 / Docker 门禁 → 系统 QQ 提醒 → 配置漂移 → SnowLuma 协议 → start
            const allowed = await requestDesktopConsent(async () => {
                clearConsentErrorSuppression(botId);
                setStartingBotId(botId);
                const runtimeGate = runtimeStartGate(botId);
                if (runtimeGate) {
                    pushInfoBar({
                        tone: 'danger',
                        title: '无法启动',
                        content: runtimeGate,
                        key: `bot-start-gate:${botId}`,
                    });
                    setStartingBotId(null);
                    return;
                }
                const gate = dockerStartGate(botId);
                if (gate) {
                    pushInfoBar({
                        tone: 'danger',
                        title: '无法启动',
                        content: gate,
                        key: `bot-start-gate:${botId}`,
                    });
                    setStartingBotId(null);
                    return;
                }
                if (await needsSystemQqWarning(botId)) {
                    setSystemQqBotId(botId);
                    setStartingBotId(null);
                    return;
                }
                await startAfterQqCheck(botId);
            });
            if (!allowed) return;
        },
        [
            clearConsentErrorSuppression,
            dockerStartGate,
            needsSystemQqWarning,
            runtimeStartGate,
            startAfterQqCheck,
        ],
    );

    const handleSystemQqContinue = useCallback(
        (dismissForever: boolean) => {
            const botId = systemQqBotId;
            setSystemQqBotId(null);
            if (dismissForever) dismissSystemQqWarning();
            if (botId) startAfterQqCheck(botId).catch(() => undefined);
        },
        [startAfterQqCheck, systemQqBotId],
    );

    const handleSystemQqInstall = useCallback(
        (dismissForever: boolean) => {
            setSystemQqBotId(null);
            if (dismissForever) dismissSystemQqWarning();
            onNavigate?.('components');
        },
        [onNavigate],
    );

    const handleDriftConfirm = useCallback(
        async (decisions: DriftDecision[]) => {
            if (!driftBotId) return;
            clearConsentErrorSuppression(driftBotId);
            setStartingBotId(driftBotId);
            const runtimeGate = runtimeStartGate(driftBotId);
            if (runtimeGate) {
                setPendingDrift(null);
                pushInfoBar({
                    tone: 'danger',
                    title: '无法启动',
                    content: runtimeGate,
                    key: `bot-start-gate:${driftBotId}`,
                });
                setDriftBotId(null);
                setStartingBotId(null);
                return;
            }
            const gate = dockerStartGate(driftBotId);
            if (gate) {
                setPendingDrift(null);
                pushInfoBar({
                    tone: 'danger',
                    title: '无法启动',
                    content: gate,
                    key: `bot-start-gate:${driftBotId}`,
                });
                setDriftBotId(null);
                setStartingBotId(null);
                return;
            }
            setPendingDrift(null);
            try {
                const ready = await prepareSnowLumaConsentOrOpen(driftBotId, decisions);
                if (!ready) {
                    setDriftBotId(null);
                    return;
                }
                const snap = await botService.startWithDecisions(driftBotId, decisions);
                pushInfoBar({
                    tone: 'success',
                    title: '操作完成',
                    content: `已发送启动指令给 Bot: ${snap.bot_id}`,
                    autoDismissMs: 4000,
                });
            } catch (err: unknown) {
                if (isSnowLumaConsentError(err)) {
                    await openSnowLumaConsent(driftBotId, decisions);
                    setDriftBotId(null);
                    return;
                }
                pushErrorBar({
                    title: '启动失败',
                    raw: errorText(err),
                });
            }
            setDriftBotId(null);
            setStartingBotId(null);
        },
        [
            clearConsentErrorSuppression,
            driftBotId,
            dockerStartGate,
            runtimeStartGate,
            openSnowLumaConsent,
            prepareSnowLumaConsentOrOpen,
        ],
    );

    const handleConsentConfirm = useCallback(async () => {
        if (!consentBotId || !consentPayload) return;
        setConsentSubmitting(true);
        const retryBotId = consentBotId;
        const retryDecisions = consentRetryDecisions;
        try {
            await botService.acceptSnowLumaAgreements(retryBotId, consentPayload.version);
            suppressCurrentConsentError(retryBotId);
        } catch (err: unknown) {
            pushErrorBar({
                title: 'SnowLuma 协议确认失败',
                raw: errorText(err),
            });
            setConsentSubmitting(false);
            return;
        }

        setConsentBotId(null);
        setConsentPayload(null);
        setConsentRetryDecisions(null);
        activeConsentBotRef.current = null;
        setConsentSubmitting(false);

        if (retryDecisions) {
            try {
                setStartingBotId(retryBotId);
                const ready = await prepareSnowLumaConsentOrOpen(retryBotId, retryDecisions);
                if (!ready) {
                    setStartingBotId(null);
                    return;
                }
                const snap = await botService.startWithDecisions(retryBotId, retryDecisions);
                pushInfoBar({
                    tone: 'success',
                    title: '操作完成',
                    content: `已发送启动指令给 Bot: ${snap.bot_id}`,
                    autoDismissMs: 4000,
                });
            } catch (err: unknown) {
                pushErrorBar({
                    title: '启动失败',
                    raw: errorText(err),
                });
            } finally {
                setStartingBotId(null);
            }
        } else {
            startBotDirect(retryBotId).catch(() => undefined);
        }
    }, [
        consentBotId,
        consentPayload,
        consentRetryDecisions,
        prepareSnowLumaConsentOrOpen,
        startBotDirect,
        suppressCurrentConsentError,
    ]);

    const handleConsentCancel = useCallback(() => {
        if (consentSubmitting) return;
        const botId = consentBotId;
        if (botId) suppressCurrentConsentError(botId);
        setConsentBotId(null);
        setConsentPayload(null);
        setConsentRetryDecisions(null);
        activeConsentBotRef.current = null;
        if (botId) {
            botService.releaseSnowLumaAgreementSession(botId).catch(() => undefined);
        }
        setStartingBotId(null);
    }, [consentBotId, consentSubmitting, suppressCurrentConsentError]);

    const handleDriftCancel = useCallback(() => {
        setPendingDrift(null);
        setDriftBotId(null);
    }, []);

    useEffect(() => {
        if (activeConsentBotRef.current || openingConsentBotRef.current) return;
        const pending = botSnapshots.find((bot) => {
            const lastError = bot.last_error?.trim();
            if (!lastError || !isSnowLumaConsentError(lastError)) {
                suppressedConsentErrorsRef.current.delete(bot.bot_id);
                return false;
            }
            return suppressedConsentErrorsRef.current.get(bot.bot_id) !== lastError;
        });
        if (!pending) return;
        void openSnowLumaConsent(pending.bot_id);
    }, [botSnapshots, openSnowLumaConsent]);

    return {
        pendingDrift,
        consentBotId,
        consentPayload,
        consentSubmitting,
        startingBotId,
        systemQqBotId,
        setSystemQqBotId,
        handleStartBot,
        handleSystemQqContinue,
        handleSystemQqInstall,
        handleDriftConfirm,
        handleDriftCancel,
        handleConsentConfirm,
        handleConsentCancel,
        clearConsentErrorSuppression,
        prepareSnowLumaConsentOrOpen,
    };
}

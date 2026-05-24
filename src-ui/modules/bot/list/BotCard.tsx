import React, { useEffect, useState } from 'react';
import { Card, Text, Button, Badge, Tooltip } from '@fluentui/react-components';
import {
    PlayFilled,
    StopFilled,
    SettingsRegular,
    TagRegular,
    HistoryRegular,
    ErrorCircleRegular,
    DocumentRegular,
    GlobeRegular,
} from '@fluentui/react-icons';
import { BotActorSnapshot, NapCatLoginInvalidationReason, DaemonState, SnowLumaLoginState } from '../../../core/ipc/types';
import {
    botStateBadge,
    canStartBot,
    canStopBot,
    isBotRunning,
    isBotStarting,
} from '../../../core/domain/bot/status';
import {
    isWebuiAvailable,
    webuiTooltip,
    type NapcatWebuiBinding,
} from '../../../core/domain/webui/availability';
import type { Flavor } from '../../../core/domain/bot/flavor';
import { useOpenWebui } from '../../../hooks/webui/useOpenWebui';
import './BotCard.css';

interface BotCardProps {
    bot: BotActorSnapshot;
    flavor: Flavor | null;
    onStart: (botId: string) => void;
    onStop: (botId: string) => void;
    onConfigure: (botId: string) => void;
    onViewLogs: (botId: string) => void;
    isBatchMode: boolean;
    isSelected: boolean;
    onToggleSelect: (botId: string) => void;
    /** NapCat WebUI 登录二维码 (data URL 或 普通 URL)，非空时渲染二维码区域。 */
    qrcodeUrl?: string | null;
    /** Bot 是否在线。`true` 显示在线徽章，`false` 显示离线徽章，`null/undefined` 不显示。 */
    isOnline?: boolean | null;
    /** 登录失效原因，`'kicked'` 时弹出一次性踢线提示（3 秒自动消失）。 */
    invalidationReason?: NapCatLoginInvalidationReason | null;
    /** NapCat WebUI port + token（事件聚合产物）。 */
    napcatBinding?: NapcatWebuiBinding | null;
    /** SnowLuma 系列状态。 */
    isSnowLuma?: boolean;
    snowlumaDaemonState?: DaemonState | null;
    snowlumaInjected?: boolean;
    snowlumaUin?: string | null;
    snowlumaLoginState?: SnowLumaLoginState | null;
}

export const BotCard: React.FC<BotCardProps> = ({
    bot,
    flavor,
    onStart,
    onStop,
    onConfigure,
    onViewLogs,
    isBatchMode,
    isSelected,
    onToggleSelect,
    qrcodeUrl,
    isOnline,
    invalidationReason,
    napcatBinding,
    isSnowLuma,
    snowlumaDaemonState,
    snowlumaInjected,
    snowlumaUin,
    snowlumaLoginState,
}) => {
    const openWebui = useOpenWebui();

    // 踢线提示自动隐藏：每次 invalidationReason 变更触发 3s 计时器
    const [showKickedToast, setShowKickedToast] = useState(false);
    useEffect(() => {
        if (invalidationReason === 'kicked') {
            setShowKickedToast(true);
            const timer = setTimeout(() => setShowKickedToast(false), 3000);
            return () => clearTimeout(timer);
        }
        setShowKickedToast(false);
        return undefined;
    }, [invalidationReason]);

    const handleCardClick = () => {
        if (isBatchMode) {
            onToggleSelect(bot.bot_id);
        }
    };

    const badgeInfo = botStateBadge(bot.state);
    const hasQrcode = !!qrcodeUrl;
    const isSnowLumaFlavor = isSnowLuma === true;

    const webuiAvailable = isWebuiAvailable({
        flavor,
        napcat: napcatBinding,
        snowlumaDaemonState,
    });
    const tooltipText = webuiTooltip({ flavor, available: webuiAvailable });

    const handleOpenWebui = async () => {
        try {
            await openWebui({
                botId: bot.bot_id,
                flavor,
                napcat: napcatBinding,
            });
        } catch (err) {
            // SnowLuma 路径有具体提示；NapCat 路径不大可能失败（ openExternalUrl 报错）。
            if (isSnowLumaFlavor) {
                alert(`打开 SnowLuma WebUI 失败: ${String(err)}`);
            }
        }
    };

    return (
        <Card
            className={`ndf-bot-card ${isBatchMode ? 'batch-mode' : ''} ${isSelected ? 'selected' : ''} state-${bot.state.toLowerCase()}`}
            onClick={handleCardClick}
        >
            <div className="ndf-card-accent-bar" />

            <div className="ndf-card-header-row">
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <Text weight="semibold" size={300} style={{ fontFamily: 'var(--ndf-font-mono)' }}>
                        QID: {bot.bot_id}
                    </Text>
                    <Badge color={badgeInfo.color as any} size="small" appearance="filled">
                        {badgeInfo.label}
                    </Badge>
                    {bot.pending_restart && (
                        <Tooltip content="有挂起的重启任务" relationship="label">
                            <Badge color="warning" size="small" appearance="outline">
                                待重启
                            </Badge>
                        </Tooltip>
                    )}

                    {/* SnowLuma 系列徽章。
                        优先级：daemon Crashed > daemon Starting > 已注入但无登录态 > 4 档登录态。
                        所有 SL 徽章只在 SnowLuma flavor bot 上显示。 */}
                    {isSnowLumaFlavor && snowlumaDaemonState === 'crashed' && (
                        <Badge color="danger" size="small" appearance="filled">
                            SnowLuma daemon 已崩溃，请重启 App
                        </Badge>
                    )}
                    {isSnowLumaFlavor && snowlumaDaemonState === 'starting' && (
                        <Badge color="brand" size="small" appearance="filled">
                            等待 SnowLuma daemon 就绪…
                        </Badge>
                    )}
                    {isSnowLumaFlavor
                        && snowlumaDaemonState !== 'crashed'
                        && snowlumaDaemonState !== 'starting'
                        && snowlumaInjected
                        && (snowlumaLoginState === null || snowlumaLoginState === undefined) && (
                            <Badge color="subtle" size="small" appearance="outline">
                                已注入，等待登录态…
                            </Badge>
                        )}
                    {isSnowLumaFlavor && snowlumaLoginState === 'logged_in' && (
                        <span className="ndf-online-indicator ndf-online-indicator-online">
                            <span className="ndf-online-dot ndf-online-dot-online" />
                            <Text size={100} weight="semibold">
                                在线{snowlumaUin ? ` · ${snowlumaUin}` : ''}
                            </Text>
                        </span>
                    )}
                    {isSnowLumaFlavor && snowlumaLoginState === 'waiting_for_qr_scan' && (
                        <Badge color="warning" size="small" appearance="filled">
                            等待扫码登录
                        </Badge>
                    )}
                    {isSnowLumaFlavor && snowlumaLoginState === 'starting' && (
                        <Badge color="brand" size="small" appearance="outline">
                            连接中
                        </Badge>
                    )}
                    {isSnowLumaFlavor && snowlumaLoginState === 'disconnected' && (
                        <Badge color="subtle" size="small" appearance="outline">
                            已断开
                        </Badge>
                    )}

                    {/* NapCat 在线 / 离线徽章 */}
                    {isOnline === true && (
                        <span className="ndf-online-indicator ndf-online-indicator-online">
                            <span className="ndf-online-dot ndf-online-dot-online" />
                            <Text size={100} weight="semibold">在线</Text>
                        </span>
                    )}
                    {isOnline === false && (
                        <span className="ndf-online-indicator ndf-online-indicator-offline">
                            <span className="ndf-online-dot ndf-online-dot-offline" />
                            <Text size={100} weight="semibold">离线</Text>
                        </span>
                    )}
                    {(isOnline === null || isOnline === undefined) && webuiAvailable && !isSnowLumaFlavor && (
                        <span className="ndf-online-indicator ndf-online-indicator-offline">
                            <span className="ndf-online-dot ndf-online-dot-offline" />
                            <Text size={100} weight="semibold">等待登录态</Text>
                        </span>
                    )}
                </div>

                {!isBatchMode && (
                    <div className="ndf-card-actions" onClick={(e) => e.stopPropagation()}>
                        {isBotRunning(bot.state) || isBotStarting(bot.state) ? (
                            <Tooltip content="停止 Bot" relationship="label">
                                <Button
                                    icon={<StopFilled style={{ color: '#bc2f32' }} />}
                                    size="small"
                                    appearance="subtle"
                                    disabled={!canStopBot(bot.state)}
                                    onClick={() => onStop(bot.bot_id)}
                                />
                            </Tooltip>
                        ) : (
                            <Tooltip content="启动 Bot" relationship="label">
                                <Button
                                    icon={<PlayFilled style={{ color: '#107c41' }} />}
                                    size="small"
                                    appearance="subtle"
                                    disabled={!canStartBot(bot.state)}
                                    onClick={() => onStart(bot.bot_id)}
                                />
                            </Tooltip>
                        )}
                        <Tooltip content="查看日志" relationship="label">
                            <Button
                                icon={<DocumentRegular />}
                                size="small"
                                appearance="subtle"
                                onClick={() => onViewLogs(bot.bot_id)}
                            />
                        </Tooltip>
                        <Tooltip content={tooltipText} relationship="label">
                            <Button
                                icon={<GlobeRegular />}
                                size="small"
                                appearance="subtle"
                                disabled={!webuiAvailable}
                                onClick={handleOpenWebui}
                            />
                        </Tooltip>
                        <Tooltip content="配置信息" relationship="label">
                            <Button
                                icon={<SettingsRegular />}
                                size="small"
                                appearance="subtle"
                                onClick={() => onConfigure(bot.bot_id)}
                            />
                        </Tooltip>
                    </div>
                )}
            </div>

            {showKickedToast && (
                <div className="ndf-kicked-toast" role="status">
                    <Text size={100} weight="semibold">Bot 账号被踢，正在重新登录…</Text>
                </div>
            )}

            <div className="ndf-card-body-row">
                <div className="ndf-bot-avatar-wrapper">
                    {hasQrcode ? (
                        <Tooltip content="使用 QQ 扫码登录" relationship="label">
                            <div className="ndf-bot-qrcode-container">
                                <img
                                    className="ndf-bot-qrcode-img"
                                    src={qrcodeUrl!}
                                    alt="WebUI 登录二维码"
                                />
                            </div>
                        </Tooltip>
                    ) : (
                        <div className="ndf-bot-avatar-container">
                            <div className="ndf-bot-avatar-circle">
                                <Text size={500} weight="semibold" style={{ color: 'var(--colorBrandForegroundLink)' }}>
                                    {bot.bot_id.slice(-2)}
                                </Text>
                            </div>
                        </div>
                    )}
                </div>

                <div className="ndf-bot-info-pane">
                    <div className="ndf-info-item">
                        <HistoryRegular className="ndf-info-icon" />
                        <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginRight: '6px' }}>修订版本:</Text>
                        <Text size={100} weight="semibold" style={{ fontFamily: 'var(--ndf-font-mono)' }}>
                            r{bot.revision} (代数: {bot.token_generation})
                        </Text>
                    </div>

                    <div className="ndf-info-item" style={{ display: 'flex', alignItems: 'flex-start' }}>
                        <TagRegular className="ndf-info-icon" style={{ marginTop: '2px' }} />
                        <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginRight: '6px' }}>最后动作:</Text>
                        <Text size={100} weight="semibold" style={{ wordBreak: 'break-all', fontFamily: 'var(--ndf-font-mono)' }}>
                            {bot.last_transition || '无'}
                        </Text>
                    </div>

                    {bot.last_error && (
                        <div className="ndf-info-item" style={{ display: 'flex', alignItems: 'flex-start', color: '#bc2f32' }}>
                            <ErrorCircleRegular className="ndf-info-icon" style={{ color: '#bc2f32', marginTop: '2px' }} />
                            <Text size={100} style={{ color: '#bc2f32', marginRight: '6px' }}>最后错误:</Text>
                            <Text size={100} weight="semibold" style={{ wordBreak: 'break-all', fontFamily: 'var(--ndf-font-mono)' }}>
                                {bot.last_error}
                            </Text>
                        </div>
                    )}
                </div>
            </div>
        </Card>
    );
};

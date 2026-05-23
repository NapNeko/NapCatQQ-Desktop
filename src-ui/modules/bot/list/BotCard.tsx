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
import { openUrl } from '@tauri-apps/plugin-opener';
import { BotActorSnapshot, NapCatLoginInvalidationReason } from '../../../core/ipc/types';
import './BotCard.css';

interface BotCardProps {
  bot: BotActorSnapshot;
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
  /** NapCat WebUI 监听端口（由 BotListPage 从 napcat_webui_available 事件聚合）。 */
  webuiPort?: number | null;
  /** NapCat WebUI 一次性 token（由 BotListPage 从 napcat_webui_available 事件聚合）。 */
  webuiToken?: string | null;
}

export const BotCard: React.FC<BotCardProps> = ({
  bot,
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
  webuiPort,
  webuiToken,
}) => {
  // 后端 BotActorState 用 #[serde(rename_all = "snake_case")] 序列化，
  // 所以前端拿到的是 "running" / "starting" / ... 而非 "Running"。
  const isRunning = bot.state === 'running';
  const isStarting = bot.state === 'starting';
  const isStopping = bot.state === 'stopping';
  const isRepairing = bot.state === 'repairing';

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

  // Handle card click in batch mode
  const handleCardClick = () => {
    if (isBatchMode) {
      onToggleSelect(bot.bot_id);
    }
  };

  const getBadgeAppearanceAndColor = () => {
    // 与后端 snake_case 序列化对齐。
    switch (bot.state) {
      case 'running':
        return { color: 'success', label: '运行中' };
      case 'starting':
        return { color: 'brand', label: '启动中' };
      case 'stopping':
        return { color: 'warning', label: '停止中' };
      case 'stopped':
        return { color: 'tiny', label: '已停止' };
      case 'crashed':
        return { color: 'danger', label: '崩溃' };
      case 'repairing':
        return { color: 'warning', label: '修复中' };
      default:
        return { color: 'neutral', label: bot.state };
    }
  };

  const badgeInfo = getBadgeAppearanceAndColor();

  const hasQrcode = !!qrcodeUrl;
  // WebUI HTTP 服务在 napcat_webui_available 事件到达时就已就绪——这跟 QQ 是否
  // 登录无关。用户恰恰需要先打开 WebUI 去扫码，所以按钮启用条件只看 port/token，
  // 不看 isOnline。
  const webuiAvailable = typeof webuiPort === 'number' && !!webuiToken;
  const webuiHref = webuiAvailable
    ? `http://127.0.0.1:${webuiPort}/webui?token=${encodeURIComponent(webuiToken!)}`
    : undefined;
  const webuiTooltip = webuiAvailable
    ? '在浏览器中打开 NapCat WebUI'
    : 'WebUI 链接将在 Bot 启动后可用';

  return (
    <Card
      className={`ndf-bot-card ${isBatchMode ? 'batch-mode' : ''} ${isSelected ? 'selected' : ''} state-${bot.state.toLowerCase()}`}
      onClick={handleCardClick}
    >
      {/* State Left Accent Indicator Bar */}
      <div className="ndf-card-accent-bar" />

      {/* Header layout row */}
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
          {/* 在线 / 离线徽章。
              - isOnline===true：QQ 账号已登录（来自 NapCat GetQQLoginInfo.online）
              - isOnline===false：进程已就绪但 QQ 未登录（等扫码或被踢）
              - isOnline===null/undefined：Poller 还没收到首轮响应；
                此时若 webui 已 available（port/token 在）显示「等待登录态」，
                否则保持空白让 Bot 状态 Badge 兜底。 */}
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
          {(isOnline === null || isOnline === undefined) && webuiAvailable && (
            <span className="ndf-online-indicator ndf-online-indicator-offline">
              <span className="ndf-online-dot ndf-online-dot-offline" />
              <Text size={100} weight="semibold">等待登录态</Text>
            </span>
          )}
        </div>

        {/* Header Action Buttons (Only visible in non-batch mode) */}
        {!isBatchMode && (
          <div className="ndf-card-actions" onClick={(e) => e.stopPropagation()}>
            {isRunning || isStarting ? (
              <Tooltip content="停止 Bot" relationship="label">
                <Button
                  icon={<StopFilled style={{ color: '#bc2f32' }} />}
                  size="small"
                  appearance="subtle"
                  disabled={isStopping}
                  onClick={() => onStop(bot.bot_id)}
                />
              </Tooltip>
            ) : (
              <Tooltip content="启动 Bot" relationship="label">
                <Button
                  icon={<PlayFilled style={{ color: '#107c41' }} />}
                  size="small"
                  appearance="subtle"
                  disabled={isStarting || isRepairing}
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
            <Tooltip content={webuiTooltip} relationship="label">
              <Button
                icon={<GlobeRegular />}
                size="small"
                appearance="subtle"
                disabled={!webuiAvailable}
                onClick={() => {
                  if (webuiHref) {
                    // Tauri webview 不支持 target=_blank；走 plugin-opener
                    // 由系统默认浏览器打开 NapCat WebUI。
                    openUrl(webuiHref).catch((err) => {
                      console.error('打开 NapCat WebUI 失败:', err);
                    });
                  }
                }}
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

      {/* 踢线提示 toast：黄色背景，3 秒后自动消失 */}
      {showKickedToast && (
        <div className="ndf-kicked-toast" role="status">
          <Text size={100} weight="semibold">Bot 账号被踢，正在重新登录…</Text>
        </div>
      )}

      {/* Card Technical details pane */}
      <div className="ndf-card-body-row">
        {/* Left: Interactive Avatar / WebUI 二维码 */}
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

        {/* Right: Technical monitoring items */}
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


import React from 'react';
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
import { BotActorSnapshot } from '../../../core/ipc/types';
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
}) => {
  const isRunning = bot.state === 'Running';
  const isStarting = bot.state === 'Starting';
  const isStopping = bot.state === 'Stopping';
  const isRepairing = bot.state === 'Repairing';

  // Handle card click in batch mode
  const handleCardClick = () => {
    if (isBatchMode) {
      onToggleSelect(bot.bot_id);
    }
  };

  const getBadgeAppearanceAndColor = () => {
    switch (bot.state) {
      case 'Running':
        return { color: 'success', label: '运行中' };
      case 'Starting':
        return { color: 'brand', label: '启动中' };
      case 'Stopping':
        return { color: 'warning', label: '停止中' };
      case 'Stopped':
        return { color: 'tiny', label: '已停止' };
      case 'Crashed':
        return { color: 'danger', label: '崩溃' };
      case 'Repairing':
        return { color: 'warning', label: '修复中' };
      default:
        return { color: 'neutral', label: bot.state };
    }
  };

  const badgeInfo = getBadgeAppearanceAndColor();

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
            <Tooltip content="WebUI 链接将在运行时接入后可用" relationship="label">
              <Button
                icon={<GlobeRegular />}
                size="small"
                appearance="subtle"
                disabled
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

      {/* Card Technical details pane */}
      <div className="ndf-card-body-row">
        {/* Left: Interactive Avatar */}
        <div className="ndf-bot-avatar-wrapper">
          <div className="ndf-bot-avatar-container">
            <div className="ndf-bot-avatar-circle">
              <Text size={500} weight="semibold" style={{ color: 'var(--colorBrandForegroundLink)' }}>
                {bot.bot_id.slice(-2)}
              </Text>
            </div>
          </div>
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

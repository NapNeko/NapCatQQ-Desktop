import React from 'react';
import { Card, Text, Button, Badge, Tooltip } from '@fluentui/react-components';
import {
  PlayFilled,
  StopFilled,
  SettingsRegular,
  ClockRegular,
  GaugeRegular,
  TagRegular,
} from '@fluentui/react-icons';
import { BotStatus } from '../../core/ipc/types';
import { formatBytes } from '../../shared/utils';
import './BotCard.css';

interface BotCardProps {
  bot: BotStatus;
  onStart: (botId: string) => void;
  onStop: (botId: string) => void;
  isBatchMode: boolean;
  isSelected: boolean;
  onToggleSelect: (botId: string) => void;
}

export const BotCard: React.FC<BotCardProps> = ({
  bot,
  onStart,
  onStop,
  isBatchMode,
  isSelected,
  onToggleSelect,
}) => {
  const isRunning = bot.state === 'Running';
  const isStarting = bot.state === 'Starting';

  // Format runtime
  const getRuntimeText = () => {
    if (!isRunning || !bot.started_at) return '未运行';
    const seconds = Math.floor((Date.now() - bot.started_at) / 1000);
    if (seconds < 0) return '00:00:00';
    const hrs = Math.floor(seconds / 3600).toString().padStart(2, '0');
    const mins = Math.floor((seconds % 3600) / 60).toString().padStart(2, '0');
    const secs = (seconds % 60).toString().padStart(2, '0');
    return `${hrs}:${mins}:${secs}`;
  };

  const [runtime, setRuntime] = React.useState(getRuntimeText());

  React.useEffect(() => {
    if (!isRunning) {
      setRuntime('未运行');
      return;
    }
    const interval = setInterval(() => {
      setRuntime(getRuntimeText());
    }, 1000);
    return () => clearInterval(interval);
  }, [isRunning, bot.started_at]);

  // Handle card click in batch mode
  const handleCardClick = () => {
    if (isBatchMode) {
      onToggleSelect(bot.bot_id);
    }
  };

  return (
    <Card
      className={`ndf-bot-card ${isBatchMode ? 'batch-mode' : ''} ${isSelected ? 'selected' : ''} state-${bot.state.toLowerCase()}`}
      onClick={handleCardClick}
    >
      {/* State Left Accent Indicator Bar */}
      <div className="ndf-card-accent-bar" />

      {/* Header layout row */}
      <div className="ndf-card-header-row">
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          <Text weight="semibold" size={300} style={{ fontFamily: 'var(--ndf-font-mono)' }}>
            QID: {bot.bot_id}
          </Text>
          <Badge size="small" appearance="outline">
            {bot.flavor === 'snowluma' ? 'SnowLuma' : 'NapCat'}
          </Badge>
        </div>

        {/* Header Action Buttons (Only visible in non-batch mode) */}
        {!isBatchMode && (
          <div className="ndf-card-actions" onClick={(e) => e.stopPropagation()}>
            {isRunning ? (
              <Tooltip content="停止 Bot" relationship="label">
                <Button
                  icon={<StopFilled style={{ color: '#bc2f32' }} />}
                  size="small"
                  appearance="subtle"
                  onClick={() => onStop(bot.bot_id)}
                />
              </Tooltip>
            ) : (
              <Tooltip content="启动 Bot" relationship="label">
                <Button
                  icon={<PlayFilled style={{ color: '#107c41' }} />}
                  size="small"
                  appearance="subtle"
                  disabled={isStarting}
                  onClick={() => onStart(bot.bot_id)}
                />
              </Tooltip>
            )}
            <Tooltip content="配置信息" relationship="label">
              <Button icon={<SettingsRegular />} size="small" appearance="subtle" />
            </Tooltip>
          </div>
        )}
      </div>

      {/* Main double column area (Avatar on Left, Indicators on Right) */}
      <div className="ndf-card-body-row">
        {/* Left: Interactive Avatar */}
        <div className="ndf-bot-avatar-wrapper">
          <div className="ndf-bot-avatar-container">
            {/* Round avatar placeholder representing high-fidelity layout */}
            <div className="ndf-bot-avatar-circle">
              <Text size={500} weight="semibold" style={{ color: '#0078d4' }}>
                {bot.bot_id.slice(-2)}
              </Text>
            </div>
          </div>
        </div>

        {/* Right: Technical monitoring items */}
        <div className="ndf-bot-info-pane">
          <div className="ndf-info-item">
            <ClockRegular className="ndf-info-icon" />
            <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginRight: '6px' }}>运行时长:</Text>
            <Text size={100} weight="semibold" style={{ fontFamily: 'var(--ndf-font-mono)' }}>
              {runtime}
            </Text>
          </div>

          <div className="ndf-info-item">
            <GaugeRegular className="ndf-info-icon" />
            <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginRight: '6px' }}>内存占用:</Text>
            <Text size={100} weight="semibold" style={{ fontFamily: 'var(--ndf-font-mono)' }}>
              {isRunning ? `${formatBytes(bot.memory_rss_bytes || 0)} / ${formatBytes(bot.server_total_memory_bytes || 16777216000)}` : '- MB / - MB'}
            </Text>
          </div>

          <div className="ndf-info-item" style={{ alignItems: 'flex-start' }}>
            <TagRegular className="ndf-info-icon" style={{ marginTop: '3px' }} />
            <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginRight: '6px', marginTop: '2px' }}>协议接口:</Text>
            <div className="ndf-pill-container">
              {isRunning ? (
                <>
                  <Badge appearance="filled" color="brand" className="ndf-pill">WS</Badge>
                  <Badge appearance="outline" className="ndf-pill">HTTP</Badge>
                </>
              ) : (
                <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>未连接</Text>
              )}
            </div>
          </div>
        </div>
      </div>
    </Card>
  );
};

import React from 'react';
import { Badge } from '@fluentui/react-components';
import { BotActorState, BootstrapStatus } from '../../core/ipc/types';
import { botStateBadge } from '../../core/domain/bot/status';

interface StatusBadgeProps {
  status: BotActorState | BootstrapStatus;
  size?: 'tiny' | 'extra-small' | 'small' | 'medium' | 'large';
}

type BadgeColor =
  | 'brand'
  | 'danger'
  | 'important'
  | 'informative'
  | 'severe'
  | 'subtle'
  | 'success'
  | 'warning';

interface BootstrapBadgeInfo {
  color: BadgeColor;
  text: string;
}

function bootstrapBadge(status: BootstrapStatus): BootstrapBadgeInfo {
  switch (status) {
    case 'ready':
      return { color: 'success', text: '就绪' };
    case 'migrating':
      return { color: 'warning', text: '迁移中' };
    case 'repair_required':
      return { color: 'warning', text: '需要修复' };
    case 'failed':
      return { color: 'danger', text: '失败' };
    default:
      return { color: 'informative', text: String(status) };
  }
}

const BOOTSTRAP_KINDS = new Set<BootstrapStatus>(['ready', 'migrating', 'repair_required', 'failed']);

export const StatusBadge: React.FC<StatusBadgeProps> = ({ status, size = 'medium' }) => {
  if (BOOTSTRAP_KINDS.has(status as BootstrapStatus)) {
    const info = bootstrapBadge(status as BootstrapStatus);
    return (
      <Badge color={info.color} size={size} appearance="filled">
        {info.text}
      </Badge>
    );
  }

  const badge = botStateBadge(status as BotActorState);
  // `tiny` / `neutral` 不是 Fluent Badge 合法 color；映射成兜底色。
  const safeColor: BadgeColor =
    badge.color === 'tiny' || badge.color === 'neutral'
      ? 'informative'
      : badge.color;

  return (
    <Badge color={safeColor} size={size} appearance="filled">
      {badge.label}
    </Badge>
  );
};

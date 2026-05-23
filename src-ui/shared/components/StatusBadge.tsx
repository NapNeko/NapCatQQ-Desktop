import React from 'react';
import { Badge } from '@fluentui/react-components';
import { BotActorState, BootstrapStatus } from '../../core/ipc/types';

interface StatusBadgeProps {
  status: BotActorState | BootstrapStatus;
  size?: 'tiny' | 'extra-small' | 'small' | 'medium' | 'large';
}

export const StatusBadge: React.FC<StatusBadgeProps> = ({ status, size = 'medium' }) => {
  let color: 'brand' | 'danger' | 'important' | 'informative' | 'severe' | 'subtle' | 'success' | 'warning' = 'informative';
  let text = String(status);

  switch (status) {
    case 'running':
    case 'ready':
      color = 'success';
      text = status === 'ready' ? '就绪' : '运行中';
      break;
    case 'stopped':
      color = 'informative';
      text = '已停止';
      break;
    case 'starting':
    case 'migrating':
      color = 'warning';
      text = status === 'migrating' ? '迁移中' : '启动中';
      break;
    case 'stopping':
      color = 'severe';
      text = '停止中';
      break;
    case 'repairing':
    case 'repair_required':
      color = 'warning';
      text = status === 'repair_required' ? '需要修复' : '修复中';
      break;
    case 'crashed':
    case 'failed':
      color = 'danger';
      text = status === 'failed' ? '失败' : '崩溃';
      break;
    default:
      color = 'informative';
      break;
  }

  return (
    <Badge color={color} size={size} appearance="filled">
      {text}
    </Badge>
  );
};

import React from 'react';
import { Button, Text } from '@fluentui/react-components';
import { PlayRegular, StopRegular, DismissRegular, DeleteRegular } from '@fluentui/react-icons';
import './BatchToolbar.css';

interface BatchToolbarProps {
  selectedCount: number;
  onBatchStart: () => void;
  onBatchStop: () => void;
  onBatchDelete: () => void;
  onCancel: () => void;
  isLoading: boolean;
}

export const BatchToolbar: React.FC<BatchToolbarProps> = ({
  selectedCount,
  onBatchStart,
  onBatchStop,
  onBatchDelete,
  onCancel,
  isLoading,
}) => {
  if (selectedCount === 0) return null;

  return (
    <div className="ndf-bottom-overlay-toolbar">
      <div className="ndf-bottom-toolbar-left" style={{ marginRight: '16px' }}>
        <Text weight="semibold" size={200} style={{ color: 'var(--colorNeutralForeground1)' }}>
          已选中 {selectedCount} 个 Bot 实例
        </Text>
      </div>
      <div className="ndf-bottom-toolbar-right" style={{ gap: '8px', display: 'flex' }}>
        <Button
          appearance="primary"
          size="small"
          icon={<PlayRegular />}
          onClick={onBatchStart}
          disabled={isLoading}
        >
          批量启动
        </Button>
        <Button
          appearance="secondary"
          size="small"
          icon={<StopRegular />}
          onClick={onBatchStop}
          disabled={isLoading}
        >
          批量停止
        </Button>
        <Button
          appearance="secondary"
          size="small"
          icon={<DeleteRegular />}
          onClick={onBatchDelete}
          disabled={isLoading}
          style={{ color: '#bc2f32', borderColor: '#bc2f32' }}
        >
          批量删除
        </Button>
        <Button
          appearance="subtle"
          size="small"
          icon={<DismissRegular />}
          onClick={onCancel}
          disabled={isLoading}
        >
          取消
        </Button>
      </div>
    </div>
  );
};

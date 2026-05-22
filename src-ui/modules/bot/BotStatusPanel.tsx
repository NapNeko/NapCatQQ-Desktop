import React, { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Card,
  Text,
  Spinner,
  MessageBar,
  MessageBarBody,
  Divider,
  Input,
  Select,
  Dialog,
  DialogTrigger,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogActions,
  DialogContent,
  Tooltip,
} from '@fluentui/react-components';
import {
  PlayRegular,
  StopRegular,
  AddRegular,
  ArrowClockwiseRegular,
  CheckmarkRegular,
  DismissRegular,
  SelectAllOffRegular,
  CheckmarkCircleRegular,
} from '@fluentui/react-icons';
import { client } from '../../core/ipc/client';
import { subscribeToEvents } from '../../core/ipc/events';
import { BotFlavor } from '../../core/ipc/types';
import { BotCard } from './BotCard';
import './BotStatusPanel.css';

export const BotStatusPanel: React.FC = () => {
  const queryClient = useQueryClient();
  const [actionMessage, setActionMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Batch Mode States
  const [isBatchMode, setIsBatchMode] = useState(false);
  const [selectedBotIds, setSelectedBotIds] = useState<Set<string>>(new Set());

  // Add Bot Dialog State
  const [isAddDialogOpen, setIsAddDialogOpen] = useState(false);
  const [newBotId, setNewBotId] = useState('10003');
  const [newBotFlavor, setNewBotFlavor] = useState<BotFlavor>('napcat');
  const [launchCommand, setLaunchCommand] = useState('node napcat.mjs');

  // Query bot list
  const { data: bots = [], isLoading, error, refetch } = useQuery({
    queryKey: ['botStatuses'],
    queryFn: client.getAllBotStatuses,
  });

  // Listen for Tauri events
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    const setup = async () => {
      unsubscribe = await subscribeToEvents((event) => {
        if (event.kind === 'bot_status_changed' || event.kind === 'bot_state_changed') {
          console.log('Bot state event received, refetching statuses:', event);
          refetch();
        }
      });
    };
    setup();
    return () => {
      if (unsubscribe) unsubscribe();
    };
  }, [refetch]);

  // Mutations
  const spawnMutation = useMutation({
    mutationFn: client.spawnLocalBot,
    onSuccess: (bot) => {
      queryClient.invalidateQueries({ queryKey: ['botStatuses'] });
      setActionMessage({ type: 'success', text: `成功启动 Bot ${bot.bot_id} (PID: ${bot.pid})` });
      setIsAddDialogOpen(false);
      // Advance placeholder QID
      setNewBotId((prev) => String(Number(prev) + 1));
    },
    onError: (err: any) => {
      setActionMessage({ type: 'error', text: `启动 Bot 失败: ${err}` });
    },
  });

  const stopMutation = useMutation({
    mutationFn: client.stopLocalBot,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['botStatuses'] });
      setActionMessage({ type: 'success', text: `成功停止 Bot ${variables.bot_id}` });
    },
    onError: (err: any) => {
      setActionMessage({ type: 'error', text: `停止 Bot 失败: ${err}` });
    },
  });

  const handleStartBot = (bot_id: string) => {
    setActionMessage(null);
    spawnMutation.mutate({
      bot_id,
      flavor: 'napcat',
      launch_command: ['node', 'napcat.mjs'],
    });
  };

  const handleStopBot = (bot_id: string) => {
    setActionMessage(null);
    stopMutation.mutate({
      bot_id,
      mode: 'graceful',
    });
  };

  const handleCreateCustomBot = () => {
    setActionMessage(null);
    if (!newBotId.trim()) return;
    spawnMutation.mutate({
      bot_id: newBotId,
      flavor: newBotFlavor,
      launch_command: launchCommand.split(' '),
    });
  };

  // Toggle selection for a card in batch mode
  const handleToggleSelect = (botId: string) => {
    setSelectedBotIds((prev) => {
      const next = new Set(prev);
      if (next.has(botId)) {
        next.delete(botId);
      } else {
        next.add(botId);
      }
      return next;
    });
  };

  // Batch commands execution
  const handleBatchStart = async () => {
    setActionMessage(null);
    const targets = Array.from(selectedBotIds);
    if (targets.length === 0) return;

    let successCount = 0;
    for (const botId of targets) {
      const bot = bots.find((b) => b.bot_id === botId);
      if (bot && bot.state !== 'Running') {
        try {
          await spawnMutation.mutateAsync({
            bot_id: botId,
            flavor: bot.flavor,
            launch_command: ['node', 'napcat.mjs'],
          });
          successCount++;
        } catch (e) {
          console.error(`批量启动失败 for ${botId}:`, e);
        }
      }
    }
    setActionMessage({ type: 'success', text: `已发送批量启动指令，成功: ${successCount} 个` });
    setSelectedBotIds(new Set());
    setIsBatchMode(false);
  };

  const handleBatchStop = async () => {
    setActionMessage(null);
    const targets = Array.from(selectedBotIds);
    if (targets.length === 0) return;

    let successCount = 0;
    for (const botId of targets) {
      const bot = bots.find((b) => b.bot_id === botId);
      if (bot && bot.state === 'Running') {
        try {
          await stopMutation.mutateAsync({
            bot_id: botId,
            mode: 'graceful',
          });
          successCount++;
        } catch (e) {
          console.error(`批量停止失败 for ${botId}:`, e);
        }
      }
    }
    setActionMessage({ type: 'success', text: `已发送批量停止指令，成功: ${successCount} 个` });
    setSelectedBotIds(new Set());
    setIsBatchMode(false);
  };

  const handleSelectAll = () => {
    setSelectedBotIds(new Set(bots.map((b) => b.bot_id)));
  };

  const handleSelectNone = () => {
    setSelectedBotIds(new Set());
  };

  if (isLoading) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '60vh' }}>
        <Spinner size="large" label="正在加载本地 Bot 实例状态..." />
      </div>
    );
  }

  if (error) {
    return (
      <div style={{ padding: '20px' }}>
        <MessageBar intent="error">
          <MessageBarBody>获取 Bot 列表失败: {String(error)}</MessageBarBody>
        </MessageBar>
      </div>
    );
  }

  return (
    <div className="ndf-home-dotted-canvas ndf-bot-panel-container">
      {actionMessage && (
        <MessageBar intent={actionMessage.type === 'success' ? 'success' : 'error'} style={{ marginBottom: '12px' }}>
          <MessageBarBody>{actionMessage.text}</MessageBarBody>
        </MessageBar>
      )}

      {/* Main Grid View of Card widgets (Flow Layout) */}
      {bots.length === 0 ? (
        <Card className="fluent-card" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '40px', textAlign: 'center', margin: '0 auto', maxWidth: '600px' }}>
          <Text size={400} weight="semibold" style={{ display: 'block', marginBottom: '8px' }}>暂无 Bot 实例</Text>
          <Text size={200} style={{ color: 'var(--colorNeutralForeground4)', marginBottom: '16px' }}>
            系统检测到当前尚未配置或部署本地机器人，请点击右下角浮动圆键创建。
          </Text>
          <Button icon={<AddRegular />} appearance="primary" onClick={() => setIsAddDialogOpen(true)}>
            部署首个 Bot 实例
          </Button>
        </Card>
      ) : (
        <div className="ndf-bot-flow-grid">
          {bots.map((bot) => (
            <BotCard
              key={bot.bot_id}
              bot={bot}
              onStart={handleStartBot}
              onStop={handleStopBot}
              isBatchMode={isBatchMode}
              isSelected={selectedBotIds.has(bot.bot_id)}
              onToggleSelect={handleToggleSelect}
            />
          ))}
        </div>
      )}

      {/* 1. Floating Circular buttons (Bottom Right) */}
      <div className="ndf-floating-group">
        {/* Toggle Batch Mode */}
        <Tooltip content={isBatchMode ? "关闭多选" : "多选批量操作"} relationship="label" positioning="before">
          <Button
            appearance="subtle"
            className={`ndf-circle-btn ${isBatchMode ? 'active' : ''}`}
            icon={<CheckmarkCircleRegular style={{ fontSize: '20px' }} />}
            onClick={() => {
              setIsBatchMode(!isBatchMode);
              setSelectedBotIds(new Set());
            }}
          />
        </Tooltip>

        {/* Refresh List */}
        <Tooltip content="刷新状态" relationship="label" positioning="before">
          <Button
            appearance="subtle"
            className="ndf-circle-btn"
            icon={<ArrowClockwiseRegular style={{ fontSize: '20px' }} />}
            onClick={() => {
              setActionMessage(null);
              refetch();
            }}
          />
        </Tooltip>

        {/* Add Bot PopUp Trigger */}
        <Tooltip content="新增 Bot 实例" relationship="label" positioning="before">
          <Button
            appearance="primary"
            className="ndf-circle-btn ndf-primary-circle"
            icon={<AddRegular style={{ fontSize: '20px', color: '#ffffff' }} />}
            onClick={() => setIsAddDialogOpen(true)}
          />
        </Tooltip>
      </div>

      {/* 2. Custom Overlay Floating Bottom CommandBar Chip */}
      {isBatchMode && (
        <div className="ndf-bottom-overlay-toolbar">
          <div className="ndf-bottom-toolbar-left">
            <Text size={200} weight="semibold" style={{ color: 'var(--colorNeutralForeground3)' }}>
              已选中 {selectedBotIds.size} / {bots.length}
            </Text>
          </div>

          <Divider vertical style={{ height: '16px', margin: '0 8px' }} />

          <div className="ndf-bottom-toolbar-right">
            <Button size="small" appearance="subtle" icon={<CheckmarkRegular />} onClick={handleSelectAll}>
              全选
            </Button>
            <Button size="small" appearance="subtle" icon={<SelectAllOffRegular />} onClick={handleSelectNone}>
              取消全选
            </Button>
            <Button
              size="small"
              appearance="secondary"
              icon={<PlayRegular style={{ color: '#107c41' }} />}
              disabled={selectedBotIds.size === 0}
              onClick={handleBatchStart}
            >
              启动
            </Button>
            <Button
              size="small"
              appearance="secondary"
              icon={<StopRegular style={{ color: '#bc2f32' }} />}
              disabled={selectedBotIds.size === 0}
              onClick={handleBatchStop}
            >
              停止
            </Button>
            <Button
              size="small"
              appearance="subtle"
              icon={<DismissRegular />}
              onClick={() => {
                setIsBatchMode(false);
                setSelectedBotIds(new Set());
              }}
            >
              退出
            </Button>
          </div>
        </div>
      )}

      {/* 3. Add Custom Bot Modal/Dialog */}
      <Dialog open={isAddDialogOpen} onOpenChange={(_, data) => setIsAddDialogOpen(data.open)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>快速配置并部署本地新实例</DialogTitle>
            <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '14px', marginTop: '12px' }}>
              <div>
                <Text size={100} style={{ display: 'block', marginBottom: '4px', color: 'var(--colorNeutralForeground4)' }}>QQ 号 (Bot ID)</Text>
                <Input value={newBotId} onChange={(e) => setNewBotId(e.target.value)} size="small" style={{ width: '100%' }} />
              </div>
              <div>
                <Text size={100} style={{ display: 'block', marginBottom: '4px', color: 'var(--colorNeutralForeground4)' }}>引擎选择 (Flavor)</Text>
                <Select
                  value={newBotFlavor}
                  onChange={(e) => setNewBotFlavor(e.target.value as BotFlavor)}
                  size="small"
                  style={{ width: '100%' }}
                >
                  <option value="napcat">NapCat (Framework)</option>
                  <option value="snowluma">SnowLuma (Lite)</option>
                </Select>
              </div>
              <div>
                <Text size={100} style={{ display: 'block', marginBottom: '4px', color: 'var(--colorNeutralForeground4)' }}>启动指令 (CommandLine)</Text>
                <Input value={launchCommand} onChange={(e) => setLaunchCommand(e.target.value)} size="small" style={{ width: '100%' }} />
              </div>
            </DialogContent>
            <DialogActions style={{ marginTop: '18px' }}>
              <DialogTrigger disableButtonEnhancement>
                <Button appearance="secondary" size="small">取消</Button>
              </DialogTrigger>
              <Button
                appearance="primary"
                size="small"
                onClick={handleCreateCustomBot}
                disabled={spawnMutation.isPending}
              >
                {spawnMutation.isPending ? '正在部署...' : '拉起 Bot 实例'}
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </div>
  );
};
export default BotStatusPanel;

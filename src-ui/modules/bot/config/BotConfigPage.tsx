import React, { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Spinner,
  MessageBar,
  MessageBarBody,
  TabList,
  Tab,
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogActions,
  DialogContent,
  Text,
} from '@fluentui/react-components';
import {
  ArrowLeftRegular,
  SaveRegular,
  DeleteRegular,
} from '@fluentui/react-icons';
import { BotConfig } from '../../../core/ipc/generated/domain/BotConfig';
import { botCommands } from '../../../core/ipc/botCommands';
import { BotBasicTab } from './tabs/BotBasicTab';
import { ConnectTab } from './tabs/ConnectTab';
import { AdvancedTab } from './tabs/AdvancedTab';

interface BotConfigPageProps {
  botId: string | null;
  onBack: () => void;
}

export const createDefaultBotConfig = (): BotConfig => ({
  bot: {
    name: '',
    QQID: 0,
    musicSignUrl: '',
    autoRestartSchedule: { enable: false, time_unit: 'h', duration: 6 },
    offlineAutoRestart: false,
    runtime_target: 'local',
    backend_type: 'napcat',
  },
  connect: {
    httpServers: [],
    httpSseServers: [],
    httpClients: [],
    websocketServers: [],
    websocketClients: [],
    plugins: [],
  },
  advanced: {
    autoStart: false,
    offlineNotice: false,
    parseMultMsg: false,
    packetServer: '',
    packetBackend: 'auto',
    enableLocalFile2Url: false,
    fileLog: false,
    consoleLog: true,
    fileLogLevel: 'debug',
    consoleLogLevel: 'info',
    o3HookMode: 1,
    bypass: { hook: false, window: false, module: false, process: false, container: false, js: false },
  },
});

export const BotConfigPage: React.FC<BotConfigPageProps> = ({
  botId,
  onBack,
}) => {
  const queryClient = useQueryClient();
  const isEditMode = botId !== null;

  // Tabs navigation
  const [activeTab, setActiveTab] = useState<'basic' | 'connect' | 'advanced'>('basic');

  // Local Form state
  const [formData, setFormData] = useState<BotConfig>(createDefaultBotConfig());

  // Alerts/notifications state
  const [alertMsg, setAlertMsg] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Deletion Dialog confirm state
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);

  // Query config of editing bot
  const { data: loadedConfig, isLoading, error } = useQuery<BotConfig | null, Error>({
    queryKey: ['botConfig', botId],
    queryFn: () => (botId ? botCommands.getBotConfig(botId) : Promise.resolve(null)),
    enabled: isEditMode,
  });

  // Sync loaded configuration into local form state
  useEffect(() => {
    if (loadedConfig) {
      setFormData(loadedConfig);
    } else if (!isEditMode) {
      setFormData(createDefaultBotConfig());
    }
  }, [loadedConfig, isEditMode]);

  // Save Config Mutation
  const saveMutation = useMutation({
    mutationFn: botCommands.upsertBotConfig,
    onSuccess: (snapshot) => {
      queryClient.invalidateQueries({ queryKey: ['botSnapshots'] });
      setAlertMsg({ type: 'success', text: `成功保存并推送 Bot 实例 [${snapshot.bot_id}] 配置！` });
      setTimeout(() => {
        onBack();
      }, 1000);
    },
    onError: (err: any) => {
      setAlertMsg({ type: 'error', text: `保存配置失败: ${err.message || err}` });
    },
  });

  // Delete Config Mutation
  const deleteMutation = useMutation({
    mutationFn: botCommands.deleteBotConfig,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['botSnapshots'] });
      setIsDeleteDialogOpen(false);
      onBack();
    },
    onError: (err: any) => {
      setAlertMsg({ type: 'error', text: `删除配置失败: ${err.message || err}` });
      setIsDeleteDialogOpen(false);
    },
  });

  const handleSave = () => {
    setAlertMsg(null);

    // Basic frontend validations (no complex logic duplicated from Rust)
    if (formData.bot.QQID <= 0 || isNaN(formData.bot.QQID)) {
      setAlertMsg({ type: 'error', text: '账号 (QQ ID) 必须是一个正整数！' });
      return;
    }
    if (!formData.bot.name.trim()) {
      setAlertMsg({ type: 'error', text: '实例名称不能为空！' });
      return;
    }

    saveMutation.mutate(formData);
  };

  const handleDelete = () => {
    if (botId) {
      deleteMutation.mutate(botId);
    }
  };

  const handleFieldChange = (section: keyof BotConfig, updatedFields: any) => {
    setFormData((prev) => ({
      ...prev,
      [section]: {
        ...prev[section],
        ...updatedFields,
      },
    }));
  };

  if (isEditMode && isLoading) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center', padding: '120px 0' }}>
        <Spinner size="large" />
        <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>正在读取 Bot 实例配置文件...</Text>
      </div>
    );
  }

  if (isEditMode && error) {
    return (
      <div style={{ padding: '24px', backgroundColor: 'var(--colorPaletteRedBackground1)', border: '1px solid var(--colorPaletteRedBorder1)', borderRadius: '6px' }}>
        <Text size={300} weight="semibold" style={{ color: 'var(--colorPaletteRedForeground1)' }}>读取配置文件失败</Text>
        <Text size={200} block style={{ marginTop: '4px', color: 'var(--colorPaletteRedForeground1)' }}>{error.message}</Text>
        <Button style={{ marginTop: '16px' }} onClick={onBack}>返回列表</Button>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', minHeight: '100%' }}>
      {/* 1. Header Toolbar */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid var(--ndf-border-subtle)', paddingBottom: '12px' }}>
        <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
          <Button icon={<ArrowLeftRegular />} onClick={onBack} appearance="subtle" />
          <div>
            <Text size={400} weight="semibold">
              {isEditMode ? `编辑 Bot 实例配置 (QID: ${botId})` : '新建 Bot 实例配置'}
            </Text>
            <Text size={100} block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '2px' }}>
              对 Bot 的底层 OneBot 适配通道、引擎自愈沙箱环境等参数进行热推送设置。
            </Text>
          </div>
        </div>

        {/* Header Action buttons */}
        <div style={{ display: 'flex', gap: '8px' }}>
          {isEditMode && (
            <Button
              icon={<DeleteRegular />}
              appearance="secondary"
              style={{ color: '#bc2f32', borderColor: '#bc2f32' }}
              onClick={() => setIsDeleteDialogOpen(true)}
            >
              删除实例
            </Button>
          )}
          <Button
            icon={<SaveRegular />}
            appearance="primary"
            onClick={handleSave}
            disabled={saveMutation.isPending}
          >
            保存配置
          </Button>
        </div>
      </div>

      {/* 2. Messages Alert Area */}
      {alertMsg && (
        <MessageBar intent={alertMsg.type} style={{ width: '100%' }}>
          <MessageBarBody>{alertMsg.text}</MessageBarBody>
        </MessageBar>
      )}

      {/* 3. Tabs Navigation bar */}
      <TabList selectedValue={activeTab} onTabSelect={(_, data) => setActiveTab(data.value as any)}>
        <Tab value="basic">基本配置 (Basic)</Tab>
        <Tab value="connect">协议连接 (Connect)</Tab>
        <Tab value="advanced">高阶优化 (Advanced)</Tab>
      </TabList>

      {/* 4. Tab Panels Viewport */}
      <div style={{ flex: 1, backgroundColor: 'var(--ndf-bg-card)', border: '1px solid var(--ndf-border-subtle)', borderRadius: '8px', padding: '16px 20px', minHeight: '400px', overflowY: 'auto' }}>
        {activeTab === 'basic' && (
          <BotBasicTab
            data={formData.bot}
            onChange={(updated) => handleFieldChange('bot', updated)}
            isEditMode={isEditMode}
          />
        )}
        {activeTab === 'connect' && (
          <ConnectTab
            data={formData.connect}
            onChange={(updated) => handleFieldChange('connect', updated)}
            backendType={formData.bot.backend_type}
          />
        )}
        {activeTab === 'advanced' && (
          <AdvancedTab
            data={formData.advanced}
            onChange={(updated) => handleFieldChange('advanced', updated)}
            backendType={formData.bot.backend_type}
          />
        )}
      </div>

      {/* 5. Delete Confirmation Dialog */}
      <Dialog open={isDeleteDialogOpen} onOpenChange={(_, data) => setIsDeleteDialogOpen(data.open)}>
        <DialogSurface style={{ maxWidth: '400px' }}>
          <DialogBody>
            <DialogTitle>确认删除该实例？</DialogTitle>
            <DialogContent style={{ marginTop: '10px' }}>
              <Text>
                你即将彻底删除 Bot 实例 <Text weight="semibold" style={{ color: '#bc2f32' }}>{botId}</Text> 的全部配置文件与数据项。
              </Text>
              <Text block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '8px', fontSize: '12px' }}>
                如果该 Bot 实例目前正处于运行中状态，删除流程中会先自动强制停止它的底层进程。此操作不可撤销！
              </Text>
            </DialogContent>
            <DialogActions style={{ marginTop: '16px' }}>
              <Button appearance="secondary" onClick={() => setIsDeleteDialogOpen(false)}>
                取消
              </Button>
              <Button
                appearance="primary"
                style={{ backgroundColor: '#bc2f32', color: '#ffffff', border: 'none' }}
                onClick={handleDelete}
                disabled={deleteMutation.isPending}
              >
                彻底删除
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </div>
  );
};

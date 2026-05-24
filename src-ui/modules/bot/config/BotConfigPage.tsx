import React, { useEffect, useState } from 'react';
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
import { createDefaultBotConfig, validateBotConfig } from '../../../core/domain/bot/config-defaults';
import { useBotConfig } from '../../../hooks/bot/useBotConfig';
import { BotBasicTab } from './tabs/BotBasicTab';
import { ConnectTab } from './tabs/ConnectTab';
import { AdvancedTab } from './tabs/AdvancedTab';

interface BotConfigPageProps {
  botId: string | null;
  onBack: () => void;
}

// 兼容旧 import，转发到 domain。
export { createDefaultBotConfig } from '../../../core/domain/bot/config-defaults';

export const BotConfigPage: React.FC<BotConfigPageProps> = ({ botId, onBack }) => {
  const isEditMode = botId !== null;

  const [activeTab, setActiveTab] = useState<'basic' | 'connect' | 'advanced'>('basic');
  const [formData, setFormData] = useState<BotConfig>(createDefaultBotConfig());
  const [alertMsg, setAlertMsg] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);

  const {
    config: loadedConfig,
    isLoading,
    error,
    save,
    isSaving,
    remove,
    isDeleting,
  } = useBotConfig(botId, {
    onSaved: (savedBotId) => {
      setAlertMsg({ type: 'success', text: `成功保存并推送 Bot 实例 [${savedBotId}] 配置！` });
      setTimeout(onBack, 1000);
    },
    onDeleted: () => {
      setIsDeleteDialogOpen(false);
      onBack();
    },
    onError: (text) => {
      setAlertMsg({ type: 'error', text });
      setIsDeleteDialogOpen(false);
    },
  });

  useEffect(() => {
    if (loadedConfig) {
      setFormData(loadedConfig);
    } else if (!isEditMode) {
      setFormData(createDefaultBotConfig());
    }
  }, [loadedConfig, isEditMode]);

  const handleSave = () => {
    setAlertMsg(null);
    const validation = validateBotConfig(formData);
    if (!validation.ok) {
      setAlertMsg({ type: 'error', text: validation.reason });
      return;
    }
    save(formData);
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
            disabled={isSaving}
          >
            保存配置
          </Button>
        </div>
      </div>

      {alertMsg && (
        <MessageBar intent={alertMsg.type} style={{ width: '100%' }}>
          <MessageBarBody>{alertMsg.text}</MessageBarBody>
        </MessageBar>
      )}

      <TabList selectedValue={activeTab} onTabSelect={(_, data) => setActiveTab(data.value as any)}>
        <Tab value="basic">基本配置 (Basic)</Tab>
        <Tab value="connect">协议连接 (Connect)</Tab>
        <Tab value="advanced">高阶优化 (Advanced)</Tab>
      </TabList>

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
                onClick={remove}
                disabled={isDeleting}
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

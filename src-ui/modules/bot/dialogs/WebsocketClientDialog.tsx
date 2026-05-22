import React, { useState, useEffect } from 'react';
import {
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogContent,
  DialogActions,
  Button,
  Field,
  Input,
  Checkbox,
  Dropdown,
  Option,
  MessageBar,
  MessageBarBody,
} from '@fluentui/react-components';
import { WebsocketClientConfig } from '../../../core/ipc/generated/WebsocketClientConfig';
import { MessagePostFormat } from '../../../core/ipc/generated/MessagePostFormat';
import { WsRole } from '../../../core/ipc/generated/WsRole';

interface WebsocketClientDialogProps {
  open: boolean;
  onClose: () => void;
  onSave: (data: WebsocketClientConfig) => void;
  initialData?: WebsocketClientConfig;
  existingNames: string[];
}

export const WebsocketClientDialog: React.FC<WebsocketClientDialogProps> = ({
  open,
  onClose,
  onSave,
  initialData,
  existingNames,
}) => {
  const [formData, setFormData] = useState<WebsocketClientConfig>({
    enable: true,
    name: '',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    url: '',
    reportSelfMessage: false,
    heartInterval: 30000,
    reconnectInterval: 5000,
    role: 'Universal',
  });

  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (initialData) {
      setFormData(initialData);
    } else {
      setFormData({
        enable: true,
        name: `ws-client-${Math.floor(Math.random() * 900 + 100)}`,
        messagePostFormat: 'array',
        token: '',
        debug: false,
        url: '',
        reportSelfMessage: false,
        heartInterval: 30000,
        reconnectInterval: 5000,
        role: 'Universal',
      });
    }
    setErrorMsg(null);
  }, [initialData, open]);

  const handleSave = () => {
    setErrorMsg(null);

    const nameTrimmed = formData.name.trim();
    if (!nameTrimmed) {
      setErrorMsg('连接名称不能为空！');
      return;
    }

    const isNew = !initialData;
    const isNameChanged = initialData && initialData.name !== nameTrimmed;
    if ((isNew || isNameChanged) && existingNames.includes(nameTrimmed)) {
      setErrorMsg(`连接名称 [${nameTrimmed}] 已存在，请使用其他名称！`);
      return;
    }

    const urlTrimmed = formData.url.trim();
    if (!urlTrimmed) {
      setErrorMsg('服务端 WebSocket URL 不能为空！');
      return;
    }
    if (!urlTrimmed.startsWith('ws://') && !urlTrimmed.startsWith('wss://')) {
      setErrorMsg('WebSocket URL 必须以 ws:// 或 wss:// 开头！');
      return;
    }

    const heartNum = Number(formData.heartInterval);
    if (isNaN(heartNum) || heartNum < 1000) {
      setErrorMsg('心跳间隔不能低于 1000 毫秒！');
      return;
    }

    const reconnectNum = Number(formData.reconnectInterval);
    if (isNaN(reconnectNum) || reconnectNum < 1000) {
      setErrorMsg('重连间隔不能低于 1000 毫秒！');
      return;
    }

    onSave({
      ...formData,
      name: nameTrimmed,
      url: urlTrimmed,
      heartInterval: heartNum,
      reconnectInterval: reconnectNum,
    });
  };

  const wsRoles: { value: WsRole; label: string }[] = [
    { value: 'Universal', label: 'Universal (全双工 - 兼收 API 与事件)' },
    { value: 'Api', label: 'API (仅限发起接口调用)' },
    { value: 'Event', label: 'Event (仅向外部接收端投递事件)' },
  ];

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface style={{ maxWidth: '400px' }}>
        <DialogBody>
          <DialogTitle>{initialData ? '编辑 WebSocket 正向客户端' : '新建 WebSocket 正向客户端'}</DialogTitle>
          <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
            {errorMsg && (
              <MessageBar intent="error">
                <MessageBarBody>{errorMsg}</MessageBarBody>
              </MessageBar>
            )}

            <Checkbox
              label="启用此 WebSocket 正向客户端 (Enable)"
              checked={formData.enable}
              onChange={(_, val) => setFormData(prev => ({ ...prev, enable: !!val.checked }))}
            />

            <Field label="连接名称 (Unique Name)" required hint="用于区分不同的协议连接端点">
              <Input
                value={formData.name}
                onChange={(_, val) => setFormData(prev => ({ ...prev, name: val.value }))}
                placeholder="例如: robot-ws-client"
              />
            </Field>

            <Field label="服务端 WebSocket URL" required hint="必须以 ws:// 或 wss:// 开头">
              <Input
                value={formData.url}
                onChange={(_, val) => setFormData(prev => ({ ...prev, url: val.value }))}
                placeholder="ws://127.0.0.1:8080/onebot/v11"
              />
            </Field>

            <Field label="连接角色 (Websocket Client Role)" required>
              <Dropdown
                value={wsRoles.find(r => r.value === formData.role)?.label || formData.role}
                selectedOptions={[formData.role]}
                onOptionSelect={(_, val) => setFormData(prev => ({ ...prev, role: val.optionValue as WsRole }))}
              >
                {wsRoles.map((role) => (
                  <Option key={role.value} value={role.value}>
                    {role.label}
                  </Option>
                ))}
              </Dropdown>
            </Field>

            <Field label="心跳发送时间 (Heart Interval MS)" required>
              <Input
                type="number"
                value={String(formData.heartInterval)}
                onChange={(_, val) => setFormData(prev => ({ ...prev, heartInterval: Number(val.value) }))}
                placeholder="30000"
              />
            </Field>

            <Field label="重新连接时间 (Reconnect Interval MS)" required>
              <Input
                type="number"
                value={String(formData.reconnectInterval)}
                onChange={(_, val) => setFormData(prev => ({ ...prev, reconnectInterval: Number(val.value) }))}
                placeholder="5000"
              />
            </Field>

            <Field label="鉴权秘钥 (Token)" hint="会在 WebSocket 连接握手时作为 Authorization Header Bearer 或 URL query 附加">
              <Input
                value={formData.token}
                onChange={(_, val) => setFormData(prev => ({ ...prev, token: val.value }))}
                placeholder="留空则不附加"
              />
            </Field>

            <Field label="消息上报格式 (Message Post Format)">
              <Dropdown
                value={formData.messagePostFormat === 'array' ? 'Array (结构化数组，推荐)' : 'String (纯文本)'}
                selectedOptions={[formData.messagePostFormat]}
                onOptionSelect={(_, val) => setFormData(prev => ({ ...prev, messagePostFormat: val.optionValue as MessagePostFormat }))}
              >
                <Option value="array">Array (结构化数组，推荐)</Option>
                <Option value="string">String (纯文本)</Option>
              </Dropdown>
            </Field>

            <Checkbox
              label="上报 Bot 自身发出的消息 (Report Self Message)"
              checked={formData.reportSelfMessage}
              onChange={(_, val) => setFormData(prev => ({ ...prev, reportSelfMessage: !!val.checked }))}
            />

            <Checkbox
              label="开启调试输出 (Debug Mode)"
              checked={formData.debug}
              onChange={(_, val) => setFormData(prev => ({ ...prev, debug: !!val.checked }))}
            />
          </DialogContent>
          <DialogActions style={{ marginTop: '16px' }}>
            <Button appearance="secondary" onClick={onClose}>
              取消
            </Button>
            <Button appearance="primary" onClick={handleSave}>
              保存
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
};

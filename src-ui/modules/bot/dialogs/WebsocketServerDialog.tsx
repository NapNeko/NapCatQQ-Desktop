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
import { WebsocketServerConfig } from '../../../core/ipc/generated/WebsocketServerConfig';
import { MessagePostFormat } from '../../../core/ipc/generated/MessagePostFormat';
import { WsRole } from '../../../core/ipc/generated/WsRole';

interface WebsocketServerDialogProps {
  open: boolean;
  onClose: () => void;
  onSave: (data: WebsocketServerConfig) => void;
  initialData?: WebsocketServerConfig;
  existingNames: string[];
}

export const WebsocketServerDialog: React.FC<WebsocketServerDialogProps> = ({
  open,
  onClose,
  onSave,
  initialData,
  existingNames,
}) => {
  const [formData, setFormData] = useState<WebsocketServerConfig>({
    enable: true,
    name: '',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    host: '0.0.0.0',
    port: 3001,
    reportSelfMessage: false,
    enableForcePushEvent: true,
    heartInterval: 30000,
    path: '/',
    role: 'Universal',
  });

  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (initialData) {
      setFormData(initialData);
    } else {
      setFormData({
        enable: true,
        name: `ws-server-${Math.floor(Math.random() * 900 + 100)}`,
        messagePostFormat: 'array',
        token: '',
        debug: false,
        host: '0.0.0.0',
        port: 3001,
        reportSelfMessage: false,
        enableForcePushEvent: true,
        heartInterval: 30000,
        path: '/',
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

    const portNum = Number(formData.port);
    if (isNaN(portNum) || portNum < 1 || portNum > 65535) {
      setErrorMsg('监听端口必须为 1 - 65535 之间的整数！');
      return;
    }

    const heartNum = Number(formData.heartInterval);
    if (isNaN(heartNum) || heartNum < 1000) {
      setErrorMsg('心跳间隔不能低于 1000 毫秒！');
      return;
    }

    onSave({
      ...formData,
      name: nameTrimmed,
      port: portNum,
      heartInterval: heartNum,
    });
  };

  const wsRoles: { value: WsRole; label: string }[] = [
    { value: 'Universal', label: 'Universal (全双工 - 兼收 API 与事件)' },
    { value: 'Api', label: 'API (仅限外部发起接口调用)' },
    { value: 'Event', label: 'Event (仅向外部投递事件)' },
  ];

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface style={{ maxWidth: '400px' }}>
        <DialogBody>
          <DialogTitle>{initialData ? '编辑 WebSocket 反向服务器' : '新建 WebSocket 反向服务器'}</DialogTitle>
          <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
            {errorMsg && (
              <MessageBar intent="error">
                <MessageBarBody>{errorMsg}</MessageBarBody>
              </MessageBar>
            )}

            <Checkbox
              label="启用此 WebSocket 服务端 (Enable)"
              checked={formData.enable}
              onChange={(_, val) => setFormData(prev => ({ ...prev, enable: !!val.checked }))}
            />

            <Field label="连接名称 (Unique Name)" required hint="用于区分不同的协议连接端点">
              <Input
                value={formData.name}
                onChange={(_, val) => setFormData(prev => ({ ...prev, name: val.value }))}
                placeholder="例如: reverse-ws-api"
              />
            </Field>

            <Field label="监听 IP (Host)" required>
              <Input
                value={formData.host}
                onChange={(_, val) => setFormData(prev => ({ ...prev, host: val.value }))}
                placeholder="0.0.0.0"
              />
            </Field>

            <Field label="监听端口 (Port)" required>
              <Input
                type="number"
                value={String(formData.port)}
                onChange={(_, val) => setFormData(prev => ({ ...prev, port: Number(val.value) }))}
                placeholder="3001"
              />
            </Field>

            <Field label="监听路径 (Path)" hint="外部应用握手时的具体路径">
              <Input
                value={formData.path}
                onChange={(_, val) => setFormData(prev => ({ ...prev, path: val.value }))}
                placeholder="例如: /onebot/v11"
              />
            </Field>

            <Field label="连接角色 (Websocket Server Role)" required>
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

            <Field label="心跳维持时间 (Heart Interval MS)" required>
              <Input
                type="number"
                value={String(formData.heartInterval)}
                onChange={(_, val) => setFormData(prev => ({ ...prev, heartInterval: Number(val.value) }))}
                placeholder="30000"
              />
            </Field>

            <Field label="鉴权秘钥 (Token)" hint="外部应用连接握手时必须在 URL Query 或 Header 中携带此 Access Token">
              <Input
                value={formData.token}
                onChange={(_, val) => setFormData(prev => ({ ...prev, token: val.value }))}
                placeholder="留空则无需鉴权"
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

            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <Checkbox
                label="上报 Bot 自身发出的消息 (Report Self)"
                checked={formData.reportSelfMessage}
                onChange={(_, val) => setFormData(prev => ({ ...prev, reportSelfMessage: !!val.checked }))}
              />
              <Checkbox
                label="强制事件推送 (Force Push Event)"
                checked={formData.enableForcePushEvent}
                onChange={(_, val) => setFormData(prev => ({ ...prev, enableForcePushEvent: !!val.checked }))}
              />
            </div>

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

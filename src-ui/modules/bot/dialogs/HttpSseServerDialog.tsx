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
import { HttpSseServerConfig } from '../../../core/ipc/generated/domain/HttpSseServerConfig';
import { MessagePostFormat } from '../../../core/ipc/generated/domain/MessagePostFormat';

interface HttpSseServerDialogProps {
  open: boolean;
  onClose: () => void;
  onSave: (data: HttpSseServerConfig) => void;
  initialData?: HttpSseServerConfig;
  existingNames: string[];
}

export const HttpSseServerDialog: React.FC<HttpSseServerDialogProps> = ({
  open,
  onClose,
  onSave,
  initialData,
  existingNames,
}) => {
  const [formData, setFormData] = useState<HttpSseServerConfig>({
    enable: true,
    name: '',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    host: '0.0.0.0',
    port: 3001,
    enableCors: true,
    enableWebsocket: false,
    reportSelfMessage: false,
  });

  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (initialData) {
      setFormData(initialData);
    } else {
      setFormData({
        enable: true,
        name: `sse-server-${Math.floor(Math.random() * 900 + 100)}`,
        messagePostFormat: 'array',
        token: '',
        debug: false,
        host: '0.0.0.0',
        port: 3001,
        enableCors: true,
        enableWebsocket: false,
        reportSelfMessage: false,
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

    onSave({
      ...formData,
      name: nameTrimmed,
      port: portNum,
    });
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface style={{ maxWidth: '400px' }}>
        <DialogBody>
          <DialogTitle>{initialData ? '编辑 HTTP SSE 服务器' : '新建 HTTP SSE 服务器'}</DialogTitle>
          <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
            {errorMsg && (
              <MessageBar intent="error">
                <MessageBarBody>{errorMsg}</MessageBarBody>
              </MessageBar>
            )}

            <Checkbox
              label="启用此 SSE 推送通道 (Enable)"
              checked={formData.enable}
              onChange={(_, val) => setFormData(prev => ({ ...prev, enable: !!val.checked }))}
            />

            <Field label="连接名称 (Unique Name)" required hint="用于区分不同的协议连接端点">
              <Input
                value={formData.name}
                onChange={(_, val) => setFormData(prev => ({ ...prev, name: val.value }))}
                placeholder="例如: koishi-sse"
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

            <Field label="鉴权秘钥 (Token)" hint="Authorization Header Bearer Token">
              <Input
                value={formData.token}
                onChange={(_, val) => setFormData(prev => ({ ...prev, token: val.value }))}
                placeholder="若不需要则留空"
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
                label="允许跨域请求 (Enable CORS)"
                checked={formData.enableCors}
                onChange={(_, val) => setFormData(prev => ({ ...prev, enableCors: !!val.checked }))}
              />
              <Checkbox
                label="在端口上兼任 WebSocket 握手 (Shared WebSocket)"
                checked={formData.enableWebsocket}
                onChange={(_, val) => setFormData(prev => ({ ...prev, enableWebsocket: !!val.checked }))}
              />
              <Checkbox
                label="上报 Bot 自身发出的消息 (Report Self Message)"
                checked={formData.reportSelfMessage}
                onChange={(_, val) => setFormData(prev => ({ ...prev, reportSelfMessage: !!val.checked }))}
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

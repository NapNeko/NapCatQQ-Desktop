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
import { HttpServerConfig } from '../../../core/ipc/generated/domain/HttpServerConfig';
import { BackendType } from '../../../core/ipc/generated/domain/BackendType';
import { MessagePostFormat } from '../../../core/ipc/generated/domain/MessagePostFormat';

interface HttpServerDialogProps {
  open: boolean;
  onClose: () => void;
  onSave: (data: HttpServerConfig) => void;
  initialData?: HttpServerConfig;
  existingNames: string[];
  backendType: BackendType;
}

export const HttpServerDialog: React.FC<HttpServerDialogProps> = ({
  open,
  onClose,
  onSave,
  initialData,
  existingNames,
  backendType,
}) => {
  const isSnowLuma = backendType === 'snowluma';

  const [formData, setFormData] = useState<HttpServerConfig>({
    enable: true,
    name: '',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    host: '0.0.0.0',
    port: 3000,
    enableCors: true,
    enableWebsocket: false,
    path: '/',
  });

  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (initialData) {
      setFormData(initialData);
    } else {
      setFormData({
        enable: true,
        name: `http-server-${Math.floor(Math.random() * 900 + 100)}`,
        messagePostFormat: 'array',
        token: '',
        debug: false,
        host: '0.0.0.0',
        port: 3000,
        enableCors: true,
        enableWebsocket: false,
        path: '/',
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

    // Name collision prevention (excluding self when editing)
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
          <DialogTitle>{initialData ? '编辑 HTTP 服务器连接' : '新建 HTTP 服务器连接'}</DialogTitle>
          <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
            {errorMsg && (
              <MessageBar intent="error">
                <MessageBarBody>{errorMsg}</MessageBarBody>
              </MessageBar>
            )}

            <Checkbox
              label="启用此连接端口 (Enable)"
              checked={formData.enable}
              onChange={(_, val) => setFormData(prev => ({ ...prev, enable: !!val.checked }))}
            />

            <Field label="连接名称 (Unique Name)" required hint="用于区分不同的协议连接端点">
              <Input
                value={formData.name}
                onChange={(_, val) => setFormData(prev => ({ ...prev, name: val.value }))}
                placeholder="例如: koishi-http"
              />
            </Field>

            <Field label="监听 IP (Host)" required>
              <Input
                value={formData.host}
                onChange={(_, val) => setFormData(prev => ({ ...prev, host: val.value }))}
                placeholder="例如: 0.0.0.0"
              />
            </Field>

            <Field label="监听端口 (Port)" required>
              <Input
                type="number"
                value={String(formData.port)}
                onChange={(_, val) => setFormData(prev => ({ ...prev, port: Number(val.value) }))}
                placeholder="例如: 3000"
              />
            </Field>

            <Field label="鉴权秘钥 (Token / AccessToken)" hint="外部应用请求 API 时必须在 Authorization Header 中携带此 Token">
              <Input
                value={formData.token}
                onChange={(_, val) => setFormData(prev => ({ ...prev, token: val.value }))}
                placeholder="若不需要鉴权则留空"
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

            {isSnowLuma ? (
              <Field label="监听路径 (Path)" hint="SnowLuma 独有字段，服务端监听的具体 HTTP 路由">
                <Input
                  value={formData.path}
                  onChange={(_, val) => setFormData(prev => ({ ...prev, path: val.value }))}
                  placeholder="例如: /"
                />
              </Field>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                <Checkbox
                  label="支持跨域请求 (Enable CORS)"
                  checked={formData.enableCors}
                  onChange={(_, val) => setFormData(prev => ({ ...prev, enableCors: !!val.checked }))}
                />
                <Checkbox
                  label="在 HTTP 端口上兼任 WebSocket 握手 (Shared WebSocket)"
                  checked={formData.enableWebsocket}
                  onChange={(_, val) => setFormData(prev => ({ ...prev, enableWebsocket: !!val.checked }))}
                />
              </div>
            )}

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

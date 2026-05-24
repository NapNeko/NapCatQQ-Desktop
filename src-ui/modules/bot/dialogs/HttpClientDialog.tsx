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
import { HttpClientConfig } from '../../../core/ipc/generated/domain/HttpClientConfig';
import { BackendType } from '../../../core/ipc/generated/domain/BackendType';
import { MessagePostFormat } from '../../../core/ipc/generated/domain/MessagePostFormat';

interface HttpClientDialogProps {
  open: boolean;
  onClose: () => void;
  onSave: (data: HttpClientConfig) => void;
  initialData?: HttpClientConfig;
  existingNames: string[];
  backendType: BackendType;
}

export const HttpClientDialog: React.FC<HttpClientDialogProps> = ({
  open,
  onClose,
  onSave,
  initialData,
  existingNames,
  backendType,
}) => {
  const isSnowLuma = backendType === 'snowluma';

  const [formData, setFormData] = useState<HttpClientConfig>({
    enable: true,
    name: '',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    url: '',
    reportSelfMessage: false,
    timeoutMs: undefined,
  });

  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (initialData) {
      setFormData(initialData);
    } else {
      setFormData({
        enable: true,
        name: `http-client-${Math.floor(Math.random() * 900 + 100)}`,
        messagePostFormat: 'array',
        token: '',
        debug: false,
        url: '',
        reportSelfMessage: false,
        timeoutMs: undefined,
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
      setErrorMsg('上报 Webhook URL 不能为空！');
      return;
    }
    if (!urlTrimmed.startsWith('http://') && !urlTrimmed.startsWith('https://')) {
      setErrorMsg('上报 URL 必须以 http:// 或 https:// 开头！');
      return;
    }

    const finalData = { ...formData, name: nameTrimmed, url: urlTrimmed };
    if (!isSnowLuma) {
      delete finalData.timeoutMs;
    }

    onSave(finalData);
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface style={{ maxWidth: '400px' }}>
        <DialogBody>
          <DialogTitle>{initialData ? '编辑 HTTP 客户端 (Webhook)' : '新建 HTTP 客户端 (Webhook)'}</DialogTitle>
          <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
            {errorMsg && (
              <MessageBar intent="error">
                <MessageBarBody>{errorMsg}</MessageBarBody>
              </MessageBar>
            )}

            <Checkbox
              label="启用此 Webhook 投递端口 (Enable)"
              checked={formData.enable}
              onChange={(_, val) => setFormData(prev => ({ ...prev, enable: !!val.checked }))}
            />

            <Field label="连接名称 (Unique Name)" required hint="用于区分不同的协议连接端点">
              <Input
                value={formData.name}
                onChange={(_, val) => setFormData(prev => ({ ...prev, name: val.value }))}
                placeholder="例如: robot-webhook"
              />
            </Field>

            <Field label="接收上报的 Webhook URL" required hint="必须以 http:// 或 https:// 开头">
              <Input
                value={formData.url}
                onChange={(_, val) => setFormData(prev => ({ ...prev, url: val.value }))}
                placeholder="http://127.0.0.1:8080/webhook"
              />
            </Field>

            <Field label="上报鉴权秘钥 (Token)" hint="会在 POST 请求中作为 Authorization Bearer Token 头部下发">
              <Input
                value={formData.token}
                onChange={(_, val) => setFormData(prev => ({ ...prev, token: val.value }))}
                placeholder="无需鉴权则留空"
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

            {isSnowLuma && (
              <Field label="请求超时时间 (Timeout MS)" hint="SnowLuma 独有可选字段，上报超时控制 (毫秒)">
                <Input
                  type="number"
                  value={formData.timeoutMs !== undefined ? String(formData.timeoutMs) : ''}
                  onChange={(_, val) => setFormData(prev => ({ ...prev, timeoutMs: val.value ? Number(val.value) : undefined }))}
                  placeholder="留空则使用引擎默认值"
                />
              </Field>
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

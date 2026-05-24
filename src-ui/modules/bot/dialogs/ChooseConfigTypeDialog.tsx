import React from 'react';
import {
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogContent,
  DialogActions,
  Button,
  Text,
} from '@fluentui/react-components';
import {
  ServerRegular,
  GlobeSearchRegular,
  ChevronRightRegular,
} from '@fluentui/react-icons';
import { BackendType } from '../../../core/ipc/generated/domain/BackendType';

interface ChooseConfigTypeDialogProps {
  open: boolean;
  onClose: () => void;
  onSelect: (type: 'httpServer' | 'httpSseServer' | 'httpClient' | 'websocketServer' | 'websocketClient') => void;
  backendType: BackendType;
}

export const ChooseConfigTypeDialog: React.FC<ChooseConfigTypeDialogProps> = ({
  open,
  onClose,
  onSelect,
  backendType,
}) => {
  const isNapCat = backendType === 'napcat';

  const types = [
    {
      key: 'httpServer' as const,
      title: 'HTTP 服务器 (HTTP Server API)',
      desc: '开启本地 HTTP 端口，接收外部的 HTTP POST API 请求。',
      icon: <ServerRegular style={{ fontSize: '24px', color: 'var(--colorBrandForegroundLink)' }} />,
      enabled: true,
    },
    {
      key: 'httpSseServer' as const,
      title: 'HTTP SSE 服务器 (Server-Sent Events)',
      desc: '开启 SSE 推送服务，向连接端单向流式推送事件。',
      icon: <GlobeSearchRegular style={{ fontSize: '24px', color: 'var(--colorPaletteOrangeBorderActive)' }} />,
      enabled: isNapCat,
      unsupportedMessage: '仅 NapCat 适配器支持 SSE 推送，SnowLuma 不支持。',
    },
    {
      key: 'httpClient' as const,
      title: 'HTTP Webhook 客户端 (Webhook HTTP Post)',
      desc: '主动将 OneBot 事件通过 HTTP POST 方式异步投递给远端接收端。',
      icon: <ServerRegular style={{ fontSize: '24px', color: 'var(--colorPaletteTealBorderActive)' }} />,
      enabled: true,
    },
    {
      key: 'websocketServer' as const,
      title: 'WebSocket 反向服务器 (Reverse WS Server)',
      desc: '作为 WS 服务端，外部应用连入后可双向调用 API 与接收事件通知。',
      icon: <GlobeSearchRegular style={{ fontSize: '24px', color: 'var(--colorPaletteBerryBorderActive)' }} />,
      enabled: true,
    },
    {
      key: 'websocketClient' as const,
      title: 'WebSocket 正向客户端 (Forward WS Client)',
      desc: '主动连入远端 WebSocket 服务端，提供全双工低延迟通信。',
      icon: <ServerRegular style={{ fontSize: '24px', color: 'var(--colorPaletteLightGreenBorderActive)' }} />,
      enabled: true,
    },
  ];

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface style={{ maxWidth: '480px' }}>
        <DialogBody>
          <DialogTitle>选择协议连接通道类别</DialogTitle>
          <DialogContent style={{ display: 'flex', flexDirection: 'column', gap: '10px', marginTop: '12px' }}>
            <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginBottom: '4px' }}>
              请根据你要对接的外部程序接口（如 Koishi, Chatbot, Webhook 接收器等）选择适合的通信模型。
            </Text>

            {types.map((type) => (
              <div
                key={type.key}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '14px',
                  padding: '12px',
                  border: '1px solid var(--ndf-border-subtle)',
                  borderRadius: '8px',
                  backgroundColor: type.enabled ? 'var(--ndf-bg-card)' : 'var(--ndf-bg-window)',
                  opacity: type.enabled ? 1 : 0.6,
                  cursor: type.enabled ? 'pointer' : 'not-allowed',
                  transition: 'all 0.15s ease',
                }}
                className={type.enabled ? 'ndf-choose-row' : ''}
                onClick={() => type.enabled && onSelect(type.key)}
              >
                {type.icon}
                <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
                  <Text weight="semibold" size={200}>{type.title}</Text>
                  <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginTop: '2px' }}>
                    {type.desc}
                  </Text>
                  {!type.enabled && (
                    <Text size={100} style={{ color: 'var(--colorPaletteRedForeground1)', marginTop: '4px', fontWeight: 'bold' }}>
                      {type.unsupportedMessage}
                    </Text>
                  )}
                </div>
                {type.enabled && <ChevronRightRegular style={{ fontSize: '18px', color: 'var(--colorNeutralForeground3)' }} />}
              </div>
            ))}
          </DialogContent>
          <DialogActions style={{ marginTop: '16px' }}>
            <Button appearance="secondary" onClick={onClose}>
              取消
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
};

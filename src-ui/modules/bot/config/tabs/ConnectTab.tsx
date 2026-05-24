import React, { useState } from 'react';
import {
  Button,
  Text,
  Badge,
  Tooltip,
} from '@fluentui/react-components';
import {
  AddRegular,
  DeleteRegular,
  GlobeRegular,
  PhoneLaptopRegular,
  SettingsRegular,
} from '@fluentui/react-icons';
import { ConnectConfig } from '../../../../core/ipc/generated/domain/ConnectConfig';
import { BackendType } from '../../../../core/ipc/generated/domain/BackendType';

// Dialog imports
import { ChooseConfigTypeDialog } from '../../dialogs/ChooseConfigTypeDialog';
import { HttpServerDialog } from '../../dialogs/HttpServerDialog';
import { HttpSseServerDialog } from '../../dialogs/HttpSseServerDialog';
import { HttpClientDialog } from '../../dialogs/HttpClientDialog';
import { WebsocketServerDialog } from '../../dialogs/WebsocketServerDialog';
import { WebsocketClientDialog } from '../../dialogs/WebsocketClientDialog';

interface ConnectTabProps {
  data: ConnectConfig;
  onChange: (updated: Partial<ConnectConfig>) => void;
  backendType: BackendType;
}

type DialogType = 'choose' | 'httpServer' | 'httpSseServer' | 'httpClient' | 'websocketServer' | 'websocketClient' | null;

export const ConnectTab: React.FC<ConnectTabProps> = ({
  data,
  onChange,
  backendType,
}) => {
  // Dialog States
  const [activeDialog, setActiveDialog] = useState<DialogType>(null);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);

  // Collect all existing names for collision check
  const existingNames = [
    ...(data.httpServers || []).map((s) => s.name),
    ...(data.httpSseServers || []).map((s) => s.name),
    ...(data.httpClients || []).map((s) => s.name),
    ...(data.websocketServers || []).map((s) => s.name),
    ...(data.websocketClients || []).map((s) => s.name),
  ];

  const handleOpenAddWizard = () => {
    setEditingIndex(null);
    setActiveDialog('choose');
  };

  const handleEditConnection = (type: DialogType, idx: number) => {
    setEditingIndex(idx);
    setActiveDialog(type);
  };

  const handleDeleteConnection = (group: keyof ConnectConfig, idx: number) => {
    const list = [...(data[group] as any[])];
    list.splice(idx, 1);
    onChange({ [group]: list });
  };

  const handleSaveConnection = (group: keyof ConnectConfig, connectionData: any) => {
    const list = [...(data[group] as any[])];
    if (editingIndex !== null) {
      // Edit Mode: Replace
      list[editingIndex] = connectionData;
    } else {
      // Create Mode: Append
      list.push(connectionData);
    }
    onChange({ [group]: list });
    setActiveDialog(null);
    setEditingIndex(null);
  };

  const renderConnectionSection = (
    title: string,
    group: keyof ConnectConfig,
    dialogType: DialogType,
    items: any[],
    renderItemDetail: (item: any) => React.ReactNode
  ) => {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', marginBottom: '16px' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <Text weight="semibold" size={200}>{title} ({items.length})</Text>
        </div>

        {items.length === 0 ? (
          <div style={{ padding: '8px 12px', border: '1px dashed var(--ndf-border-subtle)', borderRadius: '6px', backgroundColor: 'var(--ndf-bg-card)' }}>
            <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>无配置项</Text>
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
            {items.map((item, idx) => (
              <div
                key={idx}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  padding: '8px 12px',
                  border: '1px solid var(--ndf-border-subtle)',
                  borderRadius: '6px',
                  backgroundColor: 'var(--ndf-bg-card)',
                }}
              >
                <div style={{ display: 'flex', gap: '10px', alignItems: 'center' }}>
                  <GlobeRegular style={{ color: 'var(--colorBrandForegroundLink)', fontSize: '16px' }} />
                  <div style={{ display: 'flex', flexDirection: 'column' }}>
                    <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                      <Text weight="semibold" size={100}>{item.base?.name || item.name || 'Unnamed'}</Text>
                      {item.base?.enable !== false && item.enable !== false ? (
                        <Badge color="success" size="tiny">启用</Badge>
                      ) : (
                        <Badge color="subtle" size="tiny">禁用</Badge>
                      )}
                    </div>
                    {renderItemDetail(item)}
                  </div>
                </div>

                <div style={{ display: 'flex', gap: '4px' }}>
                  <Tooltip content="编辑此连接" relationship="label">
                    <Button
                      icon={<SettingsRegular />}
                      appearance="subtle"
                      size="small"
                      onClick={() => handleEditConnection(dialogType, idx)}
                    />
                  </Tooltip>
                  <Tooltip content="删除此连接" relationship="label">
                    <Button
                      icon={<DeleteRegular />}
                      appearance="subtle"
                      size="small"
                      style={{ color: '#bc2f32' }}
                      onClick={() => handleDeleteConnection(group, idx)}
                    />
                  </Tooltip>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', padding: '12px 4px' }}>
      <div>
        <Text weight="semibold" size={300}>协议通道与外部连接 (OneBot v11 API Connections)</Text>
        <Text block size={100} style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px', marginBottom: '16px' }}>
          在此处配置 OneBot v11 标准下的各种 HTTP 与 WebSocket 连接通道。
        </Text>
      </div>

      {renderConnectionSection(
        'HTTP 服务器 (HTTP Server API)',
        'httpServers',
        'httpServer',
        data.httpServers || [],
        (item) => (
          <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>
            监听: http://{item.host}:{item.port}{item.path || '/'} | WebSocket: {item.enableWebsocket ? '开启' : '关闭'}
          </Text>
        )
      )}

      {backendType === 'napcat' && renderConnectionSection(
        'HTTP SSE 服务器 (HTTP Server-Sent Events)',
        'httpSseServers',
        'httpSseServer',
        data.httpSseServers || [],
        (item) => (
          <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>
            监听: http://{item.host}:{item.port} | 自言自语推送: {item.reportSelfMessage ? '开启' : '关闭'}
          </Text>
        )
      )}

      {renderConnectionSection(
        'HTTP 客户端 (HTTP Webhook Client Post)',
        'httpClients',
        'httpClient',
        data.httpClients || [],
        (item) => (
          <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', wordBreak: 'break-all' }}>
            上报 URL: {item.url} | 超时: {item.timeoutMs ? `${item.timeoutMs}ms` : '无限制'}
          </Text>
        )
      )}

      {renderConnectionSection(
        'WebSocket 服务器 (Reverse WS Server)',
        'websocketServers',
        'websocketServer',
        data.websocketServers || [],
        (item) => (
          <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>
            监听: ws://{item.host}:{item.port}{item.path || '/'} | 角色: {item.role} | 心跳: {item.heartInterval}ms
          </Text>
        )
      )}

      {renderConnectionSection(
        'WebSocket 客户端 (Forward WS Client)',
        'websocketClients',
        'websocketClient',
        data.websocketClients || [],
        (item) => (
          <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', wordBreak: 'break-all' }}>
            服务端 URL: {item.url} | 角色: {item.role} | 重连间隔: {item.reconnectInterval}ms
          </Text>
        )
      )}

      {/* Button to open Choose Type dialog */}
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: '8px',
          padding: '24px',
          border: '1px dashed var(--ndf-border-strong)',
          borderRadius: '8px',
          backgroundColor: 'var(--ndf-bg-window)',
          marginTop: '12px',
        }}
      >
        <PhoneLaptopRegular style={{ fontSize: '28px', color: 'var(--colorNeutralForeground3)' }} />
        <div style={{ textAlign: 'center' }}>
          <Text size={200} weight="semibold" style={{ color: 'var(--colorNeutralForeground1)' }}>
            新增协议连接通道
          </Text>
          <Text block size={100} style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px' }}>
            提供可视化的 HTTP/WebSocket 协议连接向导表单（HTTP 服务器、反向 WS 等 5 大类）。
          </Text>
        </div>
        <Button
          icon={<AddRegular />}
          appearance="primary"
          size="small"
          onClick={handleOpenAddWizard}
          style={{ marginTop: '4px' }}
        >
          添加连接通道
        </Button>
      </div>

      {/* ────────────────── 五类连接配置 Dialog 模态框 ────────────────── */}

      {/* 1. Choose type dialog */}
      <ChooseConfigTypeDialog
        open={activeDialog === 'choose'}
        onClose={() => setActiveDialog(null)}
        onSelect={(type) => setActiveDialog(type)}
        backendType={backendType}
      />

      {/* 2. HTTP Server dialog */}
      <HttpServerDialog
        open={activeDialog === 'httpServer'}
        onClose={() => setActiveDialog(null)}
        onSave={(itemData) => handleSaveConnection('httpServers', itemData)}
        initialData={editingIndex !== null ? data.httpServers[editingIndex] : undefined}
        existingNames={existingNames}
        backendType={backendType}
      />

      {/* 3. HTTP SSE Server dialog */}
      <HttpSseServerDialog
        open={activeDialog === 'httpSseServer'}
        onClose={() => setActiveDialog(null)}
        onSave={(itemData) => handleSaveConnection('httpSseServers', itemData)}
        initialData={editingIndex !== null ? data.httpSseServers[editingIndex] : undefined}
        existingNames={existingNames}
      />

      {/* 4. HTTP Client dialog */}
      <HttpClientDialog
        open={activeDialog === 'httpClient'}
        onClose={() => setActiveDialog(null)}
        onSave={(itemData) => handleSaveConnection('httpClients', itemData)}
        initialData={editingIndex !== null ? data.httpClients[editingIndex] : undefined}
        existingNames={existingNames}
        backendType={backendType}
      />

      {/* 5. WS Server dialog */}
      <WebsocketServerDialog
        open={activeDialog === 'websocketServer'}
        onClose={() => setActiveDialog(null)}
        onSave={(itemData) => handleSaveConnection('websocketServers', itemData)}
        initialData={editingIndex !== null ? data.websocketServers[editingIndex] : undefined}
        existingNames={existingNames}
      />

      {/* 6. WS Client dialog */}
      <WebsocketClientDialog
        open={activeDialog === 'websocketClient'}
        onClose={() => setActiveDialog(null)}
        onSave={(itemData) => handleSaveConnection('websocketClients', itemData)}
        initialData={editingIndex !== null ? data.websocketClients[editingIndex] : undefined}
        existingNames={existingNames}
      />
    </div>
  );
};

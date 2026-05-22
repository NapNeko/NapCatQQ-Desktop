import React, { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Card,
  Text,
  Input,
  Spinner,
  MessageBar,
  MessageBarBody,
  Badge,
} from '@fluentui/react-components';
import {
  CloudRegular,
  FolderRegular,
  DocumentRegular,
  ArrowLeftRegular,
  GlobeRegular,
  CheckmarkRegular,
} from '@fluentui/react-icons';
import { client } from '../../core/ipc/client';
import { StatusBadge } from '../../shared/components/StatusBadge';
import { formatBytes } from '../../shared/utils';

export const RemoteHostPanel: React.FC = () => {
  const queryClient = useQueryClient();
  const [remoteId, setRemoteId] = useState('remote-production');
  const [host, setHost] = useState('192.168.1.100');
  const [port, setPort] = useState(22);
  const [username, setUsername] = useState('root');
  const [webuiUrl, setWebuiUrl] = useState('http://192.168.1.100:6099/webui');

  const [connectedHost, setConnectedHost] = useState<any>(null);
  const [currentPath, setCurrentPath] = useState('/');
  const [selectedBotId] = useState('20001');

  // Connect mutation
  const connectMutation = useMutation({
    mutationFn: client.connectRemoteHost,
    onSuccess: (data) => {
      setConnectedHost(data);
      queryClient.invalidateQueries({ queryKey: ['remoteFiles', data.remote_id, currentPath] });
      queryClient.invalidateQueries({ queryKey: ['remoteRuntime', data.remote_id, selectedBotId] });
      queryClient.invalidateQueries({ queryKey: ['remoteWebui', data.remote_id, selectedBotId] });
    },
  });

  // Query remote files
  const { data: files = [], isLoading: filesLoading } = useQuery({
    queryKey: ['remoteFiles', connectedHost?.remote_id, currentPath],
    queryFn: () => client.listRemoteFiles(connectedHost.remote_id, currentPath),
    enabled: !!connectedHost,
  });

  // Query remote runtime status
  const { data: runtimeStatus } = useQuery({
    queryKey: ['remoteRuntime', connectedHost?.remote_id, selectedBotId],
    queryFn: () => client.getRemoteRuntimeStatus(connectedHost.remote_id, selectedBotId),
    enabled: !!connectedHost && !!selectedBotId,
  });

  // Query remote WebUI endpoint
  const { data: webuiEndpoint } = useQuery({
    queryKey: ['remoteWebui', connectedHost?.remote_id, selectedBotId],
    queryFn: () => client.getRemoteWebuiEndpoint(connectedHost.remote_id, selectedBotId),
    enabled: !!connectedHost && !!selectedBotId,
  });

  const handleConnect = () => {
    connectMutation.mutate({
      remote_id: remoteId,
      host,
      port,
      username,
      webui_url: webuiUrl,
    });
  };

  const navigateToDirectory = (dirName: string) => {
    let newPath = currentPath;
    if (newPath === '/') {
      newPath = `/${dirName}`;
    } else {
      newPath = `${newPath}/${dirName}`;
    }
    setCurrentPath(newPath);
  };

  const navigateUp = () => {
    if (currentPath === '/') return;
    const parts = currentPath.split('/');
    parts.pop();
    const newPath = parts.join('/') || '/';
    setCurrentPath(newPath);
  };

  return (
    <div className="panel-container">
      {/* Page Title & MOCK Badge */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
            <Text size={600} weight="semibold" style={{ color: '#242424' }}>
              远端主机连接与运行时管理
            </Text>
            <Badge appearance="filled" color="brand" size="medium">
              远端运行时预览 (Mock Remote Preview)
            </Badge>
          </div>
          <Text size={200} block style={{ color: '#616161', marginTop: '4px' }}>
            在此模块中，您可以模拟或管理托管于远端 Linux SSH 主机上的 NapCat 部署实例。
          </Text>
        </div>
      </div>

      <MessageBar intent="warning">
        <MessageBarBody>
          <b>开发预览公告:</b> 本页面的远端 SSH 控制模块当前处于静态原型与模拟运行状态。UI 所有事件与文件读写均为运行时 Mock 预览。
        </MessageBarBody>
      </MessageBar>

      <div style={{ display: 'flex', gap: '20px', flex: 1, minHeight: '500px' }}>
        {/* Left: Connection and SSH Setup Form */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <Text weight="semibold" size={300}>连接参数设置 (SSH configuration)</Text>
          <Card className="fluent-card" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <div>
              <Text size={100} style={{ display: 'block', marginBottom: '4px', color: '#616161' }}>远端唯一标识符 (Remote ID)</Text>
              <Input value={remoteId} onChange={(e) => setRemoteId(e.target.value)} size="small" style={{ width: '100%' }} />
            </div>
            <div>
              <Text size={100} style={{ display: 'block', marginBottom: '4px', color: '#616161' }}>主机地址 (Host IP/Domain)</Text>
              <Input value={host} onChange={(e) => setHost(e.target.value)} size="small" style={{ width: '100%' }} />
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 2fr', gap: '10px' }}>
              <div>
                <Text size={100} style={{ display: 'block', marginBottom: '4px', color: '#616161' }}>端口号 (SSH Port)</Text>
                <Input type="number" value={String(port)} onChange={(e) => setPort(Number(e.target.value))} size="small" style={{ width: '100%' }} />
              </div>
              <div>
                <Text size={100} style={{ display: 'block', marginBottom: '4px', color: '#616161' }}>用户名 (Username)</Text>
                <Input value={username} onChange={(e) => setUsername(e.target.value)} size="small" style={{ width: '100%' }} />
              </div>
            </div>
            <div>
              <Text size={100} style={{ display: 'block', marginBottom: '4px', color: '#616161' }}>WebUI 反代地址 (WebUI Url Override)</Text>
              <Input value={webuiUrl} onChange={(e) => setWebuiUrl(e.target.value)} size="small" style={{ width: '100%' }} />
            </div>

            <Button
              appearance="primary"
              icon={connectMutation.isPending ? <Spinner size="tiny" /> : <CloudRegular />}
              onClick={handleConnect}
              disabled={connectMutation.isPending}
              style={{ marginTop: '8px' }}
            >
              模拟建立 SSH 连接
            </Button>
          </Card>

          {connectedHost && (
            <Card className="fluent-card">
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '10px' }}>
                <CheckmarkRegular style={{ color: '#107c41', fontSize: '18px' }} />
                <Text weight="semibold" size={200}>已连接至 {connectedHost.remote_id}</Text>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '6px', fontSize: '12px', color: '#616161' }}>
                <div>SSH 隧道: <b>{connectedHost.username}@{connectedHost.host}:{connectedHost.port}</b></div>
                <div>WebUI 终结点: <b>{connectedHost.webui_url || '未配置'}</b></div>
              </div>
            </Card>
          )}
        </div>

        {/* Right: Remote Runtime Preview & File System Browser */}
        <div style={{ flex: 1.8, display: 'flex', flexDirection: 'column', gap: '16px' }}>
          {connectedHost ? (
            <>
              {/* Remote Runtime State Panel */}
              <Text weight="semibold" size={300}>远端容器运行时监控</Text>
              <Card className="fluent-card" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div>
                    <Text size={300} weight="semibold">远端 Bot: {selectedBotId}</Text>
                    <Text size={100} block style={{ color: '#858585', marginTop: '2px' }}>
                      运行目标: {runtimeStatus?.runtime_target || 'Linux remote_ssh'}
                    </Text>
                  </div>
                  {runtimeStatus && <StatusBadge status={runtimeStatus.status.state} />}
                </div>

                {runtimeStatus && (
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '10px', backgroundColor: '#f3f3f4', padding: '10px', borderRadius: '4px', fontSize: '12px' }}>
                    <div>
                      <Text size={100} block style={{ color: '#616161' }}>远端进程 PID</Text>
                      <Text size={200} weight="semibold" style={{ fontFamily: 'monospace' }}>{runtimeStatus.status.pid || '-'}</Text>
                    </div>
                    <div>
                      <Text size={100} block style={{ color: '#616161' }}>内存利用指标</Text>
                      <Text size={200} weight="semibold">{formatBytes(runtimeStatus.status.memory_rss_bytes)}</Text>
                    </div>
                    <div>
                      <Text size={100} block style={{ color: '#616161' }}>活跃连接连线</Text>
                      <Text size={200} weight="semibold">{runtimeStatus.status.extra?.active_connections || 0} 个连接</Text>
                    </div>
                  </div>
                )}

                <div style={{ display: 'flex', gap: '10px', marginTop: '4px' }}>
                  {webuiEndpoint?.webui_url && (
                    <Button
                      icon={<GlobeRegular />}
                      appearance="primary"
                      size="small"
                      onClick={() => window.open(webuiEndpoint.webui_url!, '_blank')}
                    >
                      安全进入远端 WebUI
                    </Button>
                  )}
                  <Button size="small">重启远端容器</Button>
                </div>
              </Card>

              {/* File Explorer Panel */}
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: '8px' }}>
                <Text weight="semibold" size={300}>远端文件浏览器 (SFTP 挂载预览)</Text>
                <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                  <Button
                    icon={<ArrowLeftRegular />}
                    disabled={currentPath === '/'}
                    onClick={navigateUp}
                    size="small"
                    appearance="subtle"
                  />
                  <Text size={100} style={{ fontFamily: 'monospace', color: '#616161' }}>{currentPath}</Text>
                </div>
              </div>

              <div className="fluent-table-container" style={{ flex: 1 }}>
                <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1.2fr', padding: '8px 12px', backgroundColor: '#f3f3f4', borderBottom: '1px solid #e0e0e1' }}>
                  <Text size={200} weight="semibold">名称</Text>
                  <Text size={200} weight="semibold">类型</Text>
                  <Text size={200} weight="semibold" style={{ textAlign: 'right' }}>大小</Text>
                </div>

                {filesLoading ? (
                  <div style={{ padding: '30px', display: 'flex', justifyContent: 'center' }}>
                    <Spinner size="small" label="正在拉取目录项..." />
                  </div>
                ) : files.length === 0 ? (
                  <div style={{ padding: '24px', textAlign: 'center', color: '#858585' }}>
                    <Text size={200}>空目录</Text>
                  </div>
                ) : (
                  files.map((file, idx) => (
                    <div
                      key={idx}
                      onDoubleClick={() => file.is_dir && navigateToDirectory(file.name)}
                      style={{
                        display: 'grid',
                        gridTemplateColumns: '2fr 1fr 1.2fr',
                        padding: '10px 12px',
                        alignItems: 'center',
                        borderBottom: '1px solid #e0e0e1',
                        cursor: file.is_dir ? 'pointer' : 'default',
                        backgroundColor: '#ffffff',
                        transition: 'background-color 0.15s ease',
                      }}
                      className={file.is_dir ? 'folder-row' : ''}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        {file.is_dir ? (
                          <FolderRegular style={{ color: '#ffb900' }} />
                        ) : (
                          <DocumentRegular style={{ color: '#0078d4' }} />
                        )}
                        <Text size={200} weight={file.is_dir ? 'semibold' : 'regular'}>
                          {file.name}
                        </Text>
                      </div>
                      <Text size={100} style={{ color: '#858585' }}>{file.is_dir ? '文件夹' : '文件'}</Text>
                      <Text size={100} style={{ textAlign: 'right', color: '#858585', fontFamily: 'monospace' }}>
                        {file.is_dir ? '-' : formatBytes(file.size)}
                      </Text>
                    </div>
                  ))
                )}
              </div>
            </>
          ) : (
            <Card className="fluent-card" style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: '16px' }}>
              <CloudRegular style={{ fontSize: '48px', color: '#858585' }} />
              <div style={{ textAlign: 'center' }}>
                <Text size={300} weight="semibold" block>远端连接未就绪</Text>
                <Text size={200} style={{ color: '#858585', marginTop: '4px' }}>请在左侧填写 SSH 登录连接表单并点击“模拟建立 SSH 连接”。</Text>
              </div>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
};

import React, { useState } from 'react';
import {
  Button,
  Card,
  CardHeader,
  Text,
  Badge,
  Spinner,
  MessageBar,
  MessageBarTitle,
  MessageBarBody,
  Divider,
  ProgressBar,
} from '@fluentui/react-components';
import {
  FolderOpenRegular,
  DocumentRegular,
  CheckmarkCircleRegular,
  WarningRegular,
  CloudRegular,
  BotRegular,
  SettingsRegular,
  DeviceEqRegular,
} from '@fluentui/react-icons';
import { StatusBadge } from '../../shared/components/StatusBadge';
import { compactPath, formatTimestamp } from '../../core/domain/bootstrap/format';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { useResourceMonitor } from '../../hooks/diagnostics/useResourceMonitor';
import './BootstrapPanel.css';

interface BootstrapPanelProps {
  onNavigate: (tab: string) => void;
}

export const BootstrapPanel: React.FC<BootstrapPanelProps> = ({ onNavigate }) => {
  const {
    bootstrap,
    isLoading,
    error,
    openDataDir,
    exportMigrationReport,
    isOpeningDir,
    isExporting,
  } = useBootstrap();
  const { cpu, ram } = useResourceMonitor();
  const [actionMessage, setActionMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const handleOpenDataDir = async () => {
    setActionMessage(null);
    try {
      const path = await openDataDir();
      setActionMessage({ type: 'success', text: `成功打开数据目录: ${path}` });
    } catch (err: any) {
      setActionMessage({ type: 'error', text: `打开数据目录失败: ${err}` });
    }
  };

  const handleExportReport = async () => {
    setActionMessage(null);
    try {
      const path = await exportMigrationReport();
      setActionMessage({ type: 'success', text: `成功导出迁移报告至: ${path}` });
    } catch (err: any) {
      setActionMessage({ type: 'error', text: `导出迁移报告失败: ${err}` });
    }
  };

  if (isLoading) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '60vh', gap: '12px' }}>
        <Spinner size="large" label="正在加载系统自检与引导状态..." />
      </div>
    );
  }

  if (error) {
    return (
      <div style={{ padding: '20px' }}>
        <MessageBar intent="error">
          <MessageBarTitle>获取自检状态失败</MessageBarTitle>
          <MessageBarBody>{String(error)}</MessageBarBody>
        </MessageBar>
      </div>
    );
  }

  const report = bootstrap?.report;

  return (
    <div className="ndf-home-dotted-canvas">
      {actionMessage && (
        <div style={{ maxWidth: '1100px', margin: '0 auto 12px auto' }}>
          <MessageBar intent={actionMessage.type === 'success' ? 'success' : 'error'}>
            <MessageBarBody>{actionMessage.text}</MessageBarBody>
          </MessageBar>
        </div>
      )}

      <div className="ndf-home-grid">
        <div className="ndf-home-col ndf-col-7">
          <Card className="fluent-card ndf-hello-card">
            <div className="ndf-hello-content">
              <div>
                <Text size={500} weight="semibold" style={{ display: 'block', color: 'var(--colorNeutralForeground1)' }}>
                  欢迎回到 NapCatQQ 桌面助手
                </Text>
                <Text size={100} style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px', display: 'block' }}>
                  应用正在以 <b>Tauri 2.0 原生沙箱底座</b> 稳健执行中，当前状态良好。
                </Text>
              </div>
              <DeviceEqRegular className="ndf-floating-icon" />
            </div>

            <div className="ndf-quick-stats-row">
              <div className="ndf-stat-item">
                <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>自检引导状态</Text>
                {bootstrap && <StatusBadge status={bootstrap.status} size="small" />}
              </div>
              <Divider vertical style={{ height: '24px' }} />
              <div className="ndf-stat-item">
                <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>数据架构版本</Text>
                <Badge size="small" appearance="outline">{bootstrap?.schema_version || 'v3'}</Badge>
              </div>
            </div>
          </Card>

          <Card className="fluent-card" style={{ cursor: 'pointer' }} onClick={() => onNavigate('remote')}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
                <CloudRegular style={{ fontSize: '28px', color: '#0078d4' }} />
                <div>
                  <Text size={300} weight="semibold" block>远端运行时与集群速览</Text>
                  <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>
                    连接远程 SSH 主机集群，无缝部署、校验或监控外部进程状态。
                  </Text>
                </div>
              </div>
              <Badge color="brand" appearance="filled">快速跳转</Badge>
            </div>
          </Card>

          {report && (
            <Card className="fluent-card">
              <CardHeader
                header={
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <CheckmarkCircleRegular style={{ color: '#107c41', fontSize: '18px' }} />
                    <Text size={300} weight="semibold">数据自检测与平滑迁移报告</Text>
                  </div>
                }
              />

              <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '10px' }}>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
                  <div className="ndf-inner-info-box">
                    <Text weight="semibold" size={100} block style={{ marginBottom: '4px' }}>历史源检测 (V2 Source)</Text>
                    {report.source ? (
                      <>
                        <Text size={100} block><b>路径:</b> {compactPath(report.source.path)}</Text>
                        <Text size={100} block><b>历史版本:</b> {report.source.detected_version}</Text>
                      </>
                    ) : (
                      <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>全新安装，无旧版痕迹。</Text>
                    )}
                  </div>

                  <div className="ndf-inner-info-box">
                    <Text weight="semibold" size={100} block style={{ marginBottom: '4px' }}>自动备份归档 (Auto Backup)</Text>
                    {report.backup ? (
                      <>
                        <Text size={100} block><b>归档:</b> {compactPath(report.backup.backup_dir)}</Text>
                        <Text size={100} block><b>生成时间:</b> {formatTimestamp(report.backup.timestamp)}</Text>
                      </>
                    ) : (
                      <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>无需备份或热升级。</Text>
                    )}
                  </div>
                </div>

                {report.rules_applied.length > 0 && (
                  <div>
                    <Text weight="semibold" size={100} block style={{ marginBottom: '6px' }}>
                      已自动应用的配置校准规则 ({report.rules_applied.length})
                    </Text>
                    <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
                      {report.rules_applied.map((rule, idx) => (
                        <Badge key={idx} appearance="filled" color="brand" size="small">
                          {rule}
                        </Badge>
                      ))}
                    </div>
                  </div>
                )}

                {report.warnings.length > 0 && (
                  <div className="ndf-warning-section">
                    <Text weight="semibold" size={100} style={{ display: 'flex', alignItems: 'center', gap: '6px', color: '#bc2f32', marginBottom: '6px' }}>
                      <WarningRegular /> 自诊断警告或注意事项 ({report.warnings.length})
                    </Text>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                      {report.warnings.map((warning, idx) => (
                        <div key={idx} className="ndf-warning-row">
                          <Text size={100} weight="semibold" style={{ color: '#bc2f32' }}>{warning.code}:</Text>
                          <Text size={100} style={{ color: 'var(--colorNeutralForeground1)' }}>{warning.message}</Text>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </Card>
          )}
        </div>

        <div className="ndf-home-col ndf-col-5">
          <Card className="fluent-card">
            <CardHeader
              header={
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <SettingsRegular style={{ fontSize: '18px', color: '#0078d4' }} />
                  <Text size={300} weight="semibold">底座编译与版本明细</Text>
                </div>
              }
            />

            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', marginTop: '10px' }}>
              <div className="ndf-detail-row">
                <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>底座版本 (App):</Text>
                <Text size={200} weight="semibold">v0.1.0-alpha.1</Text>
              </div>
              <Divider />
              <div className="ndf-detail-row">
                <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>后端核心 (Core):</Text>
                <Text size={200} weight="semibold">NapCat Core v2.0</Text>
              </div>
              <Divider />
              <div className="ndf-detail-row">
                <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>架构配置 (Build):</Text>
                <Text size={200} weight="semibold" style={{ fontFamily: 'var(--ndf-font-mono)' }}>win32-x64-release</Text>
              </div>
              <Divider />
              <div className="ndf-detail-row">
                <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>通信协议 (IPC):</Text>
                <Text size={200} weight="semibold">Tauri Events/IPC</Text>
              </div>
            </div>
          </Card>

          <Card className="fluent-card">
            <CardHeader
              header={
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <DeviceEqRegular style={{ fontSize: '18px', color: '#0078d4' }} />
                  <Text size={300} weight="semibold">系统资源监视器</Text>
                </div>
              }
            />

            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '10px' }}>
              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '4px' }}>
                  <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>CPU 占用率</Text>
                  <Text size={100} weight="semibold" style={{ fontFamily: 'var(--ndf-font-mono)' }}>{cpu}%</Text>
                </div>
                <ProgressBar value={cpu / 100} color="brand" />
              </div>

              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '4px' }}>
                  <Text size={100} style={{ color: 'var(--colorNeutralForeground4)' }}>物理内存 (RAM)</Text>
                  <Text size={100} weight="semibold" style={{ fontFamily: 'var(--ndf-font-mono)' }}>{ram}% (7.2 GB / 16.0 GB)</Text>
                </div>
                <ProgressBar value={ram / 100} color="brand" />
              </div>

              <Divider style={{ margin: '4px 0' }} />

              <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
                <Button
                  size="small"
                  appearance="primary"
                  icon={<BotRegular />}
                  onClick={() => onNavigate('bots')}
                  style={{ justifyContent: 'flex-start' }}
                >
                  进入本地 Bot 管理
                </Button>
                <Button
                  size="small"
                  appearance="secondary"
                  icon={<FolderOpenRegular />}
                  onClick={handleOpenDataDir}
                  disabled={isOpeningDir}
                  style={{ justifyContent: 'flex-start' }}
                >
                  浏览缓存数据目录
                </Button>
                <Button
                  size="small"
                  appearance="secondary"
                  icon={<DocumentRegular />}
                  onClick={handleExportReport}
                  disabled={isExporting}
                  style={{ justifyContent: 'flex-start' }}
                >
                  导出自检诊断报告
                </Button>
              </div>
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
};
export default BootstrapPanel;

import React, { useState } from 'react';
import { CustomTitleBar } from '../shared/components/CustomTitleBar';
import { SidebarNavigation } from '../shared/components/SidebarNavigation';
import { PageHeader } from '../shared/components/PageHeader';
import { StatusBar } from '../shared/components/StatusBar';

import { BootstrapPanel } from '../modules/bootstrap/BootstrapPanel';
import { BotPage } from '../modules/bot/BotPage';
import { RemoteHostPanel } from '../modules/remote/RemoteHostPanel';
import { EventPanel } from '../modules/events/EventPanel';
import { client, isTauri } from '../core/ipc/client';
import { Button, Divider, Text } from '@fluentui/react-components';
import './App.css';

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<string>('overview');
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(false);
  const [isOpeningDir, setIsOpeningDir] = useState(false);

  const handleOpenDataDir = async () => {
    setIsOpeningDir(true);
    try {
      await client.openDataDir();
    } catch (err) {
      console.error('打开数据目录失败:', err);
    } finally {
      setIsOpeningDir(false);
    }
  };

  const renderActivePanel = () => {
    switch (activeTab) {
      case 'overview':
        return <BootstrapPanel onNavigate={(tab) => setActiveTab(tab)} />;
      case 'bots':
        return <BotPage />;
      case 'remote':
        return <RemoteHostPanel />;
      case 'events':
        return <EventPanel />;
      case 'settings':
        return renderSettingsPanel();
      default:
        return <BootstrapPanel onNavigate={(tab) => setActiveTab(tab)} />;
    }
  };

  const renderSettingsPanel = () => {
    return (
      <div className="panel-container">
        <div>
          <Text size={500} weight="semibold" style={{ color: 'var(--colorNeutralForeground1)' }}>
            系统设置 (Console Settings)
          </Text>
          <Text size={100} block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px' }}>
            查看并管理本地 NapCatQQ-Desktop 客户端的全局偏好设置与系统环境信息。
          </Text>
        </div>

        <div className="fluent-card" style={{ display: 'flex', flexDirection: 'column', gap: '16px', maxWidth: '600px' }}>
          <div>
            <Text size={300} weight="semibold" block style={{ marginBottom: '8px' }}>运行时底座架构 (Tauri Backend Info)</Text>
            <div style={{ display: 'grid', gridTemplateColumns: '1.5fr 2fr', gap: '12px 24px', fontSize: '13px' }}>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>客户端底座名称:</Text>
              <Text weight="semibold">NapCatQQ-Desktop (Tauri 迁移版)</Text>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>宿主环境:</Text>
              <Text weight="semibold">{isTauri ? 'Native Windows Tauri Window' : 'Standalone Standard Web View (Preview)'}</Text>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>API 通信方式:</Text>
              <Text weight="semibold">Tauri IPC ipc.invoke / emit</Text>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>前端底层实现:</Text>
              <Text weight="semibold">React 18 + Fluent UI v9 + TanStack Query</Text>
            </div>
          </div>

          <Divider />

          <div>
            <Text size={300} weight="semibold" block style={{ marginBottom: '8px' }}>核心版本详情</Text>
            <div style={{ display: 'grid', gridTemplateColumns: '1.5fr 2fr', gap: '12px 24px', fontSize: '13px' }}>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>应用主版本号 (Version):</Text>
              <Text weight="semibold">0.1.0-alpha.1</Text>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>编译配置 (Build):</Text>
              <Text weight="semibold">release-msi-win32-x64</Text>
              <Text style={{ color: 'var(--colorNeutralForeground4)' }}>数据图谱版本 (Schema):</Text>
              <Text weight="semibold">v3 (Tauri-native store)</Text>
            </div>
          </div>

          <Divider />

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '10px' }}>
            <Button
              appearance="secondary"
              size="small"
              onClick={handleOpenDataDir}
              disabled={isOpeningDir}
            >
              浏览系统缓存目录
            </Button>
            <Button appearance="primary" size="small">
              保存配置
            </Button>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="ndf-app-shell">
      {/* 1. 主体骨架 (从最顶部 y=0 延伸，全高通顶，不产生横向分界割裂) */}
      <div className="ndf-app-body-full">
        {/* 1.1 侧边栏导航 */}
        <SidebarNavigation
          activeTab={activeTab}
          onChangeTab={setActiveTab}
          collapsed={sidebarCollapsed}
          onToggleCollapse={() => setSidebarCollapsed(!sidebarCollapsed)}
        />

        {/* 1.2 主内容工作空间 */}
        <div className="ndf-main-workspace-full">
          {/* 1.2.1 紧凑页面头部 */}
          <PageHeader activeTab={activeTab} />

          {/* 1.2.2 滚动视口工作盘 */}
          <main className="ndf-panel-viewport">
            {renderActivePanel()}
          </main>
        </div>
      </div>

      {/* 2. 自绘全宽标题栏 (绝对定位，浮动在最顶部，实现无缝云母透明融合) */}
      <CustomTitleBar sidebarCollapsed={sidebarCollapsed} />

      {/* 3. 底部状态栏 */}
      <StatusBar />
    </div>
  );
};

export default App;

import React from 'react';
import { Button, Text, Tooltip } from '@fluentui/react-components';
import {
  HomeRegular,
  HomeFilled,
  BotRegular,
  BotFilled,
  CloudRegular,
  CloudFilled,
  InfoRegular,
  InfoFilled,
  SettingsRegular,
  SettingsFilled,
  NavigationRegular,
} from '@fluentui/react-icons';
import './SidebarNavigation.css';

interface SidebarNavigationProps {
  activeTab: string;
  onChangeTab: (tab: string) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

export const SidebarNavigation: React.FC<SidebarNavigationProps> = ({
  activeTab,
  onChangeTab,
  collapsed,
  onToggleCollapse,
}) => {
  const navItems = [
    {
      id: 'overview',
      label: '控制台概览',
      iconReg: <HomeRegular />,
      iconFil: <HomeFilled style={{ color: '#0078d4' }} />,
    },
    {
      id: 'bots',
      label: '本地 Bot 管理',
      iconReg: <BotRegular />,
      iconFil: <BotFilled style={{ color: '#0078d4' }} />,
    },
    {
      id: 'remote',
      label: '远端运行时',
      iconReg: <CloudRegular />,
      iconFil: <CloudFilled style={{ color: '#0078d4' }} />,
    },
    {
      id: 'events',
      label: '系统事件总线',
      iconReg: <InfoRegular />,
      iconFil: <InfoFilled style={{ color: '#0078d4' }} />,
    },
  ];

  const renderButton = (id: string, label: string, iconReg: React.ReactElement, iconFil: React.ReactElement) => {
    const isActive = activeTab === id;
    const buttonElement = (
      <Button
        appearance={isActive ? 'secondary' : 'subtle'}
        icon={isActive ? iconFil : iconReg}
        className={`ndf-nav-item ${isActive ? 'active' : ''}`}
        style={{
          justifyContent: collapsed ? 'center' : 'flex-start',
          paddingLeft: collapsed ? '0' : '12px',
        }}
        onClick={() => onChangeTab(id)}
      >
        {!collapsed && <span className="ndf-nav-item-label">{label}</span>}
      </Button>
    );

    if (collapsed) {
      return (
        <Tooltip content={label} relationship="label" positioning="after" key={id}>
          <div className="ndf-nav-item-wrapper">
            {isActive && <div className="ndf-nav-active-bar" />}
            {buttonElement}
          </div>
        </Tooltip>
      );
    }

    return (
      <div className="ndf-nav-item-wrapper" key={id}>
        {isActive && <div className="ndf-nav-active-bar" />}
        {buttonElement}
      </div>
    );
  };

  return (
    <aside className={`ndf-sidebar ${collapsed ? 'collapsed' : ''}`}>
      {/* Sidebar Header toggle button */}
      <div className="ndf-sidebar-toggle-container" style={{ justifyContent: collapsed ? 'center' : 'flex-start' }}>
        <Button
          appearance="subtle"
          icon={<NavigationRegular />}
          onClick={onToggleCollapse}
          title={collapsed ? '展开侧边栏' : '折叠侧边栏'}
          style={{ minWidth: 'auto', padding: '6px' }}
        />
        {!collapsed && (
          <Text weight="semibold" size={100} style={{ color: '#616161', marginLeft: '12px' }}>
            菜单区
          </Text>
        )}
      </div>

      {/* Main Navigation Items */}
      <nav className="ndf-sidebar-nav">
        {navItems.map((item) => renderButton(item.id, item.label, item.iconReg, item.iconFil))}
      </nav>

      {/* Settings at the bottom */}
      <div className="ndf-sidebar-footer">
        {renderButton(
          'settings',
          '系统设置',
          <SettingsRegular />,
          <SettingsFilled style={{ color: '#0078d4' }} />
        )}
      </div>
    </aside>
  );
};
export default SidebarNavigation;

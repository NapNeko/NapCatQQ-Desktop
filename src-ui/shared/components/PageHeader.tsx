import React, { useState } from 'react';
import { Text, Button, Spinner, Badge } from '@fluentui/react-components';
import { FolderOpenRegular } from '@fluentui/react-icons';
import { client } from '../../core/ipc/client';
import './PageHeader.css';

interface PageHeaderProps {
  activeTab: string;
}

export const PageHeader: React.FC<PageHeaderProps> = ({ activeTab }) => {
  const [isOpeningDir, setIsOpeningDir] = useState(false);

  const getPageInfo = () => {
    switch (activeTab) {
      case 'overview':
        return {
          title: '控制台概览',
          description: '诊断本地/远端系统引导状态，进行数据架构热校准。',
          tag: 'SYSTEM CORE',
        };
      case 'bots':
        return {
          title: '本地 Bot 实例管理',
          description: '管理本机运行的 NapCat / SnowLuma QQ 机器人进程与运行指标。',
          tag: 'BOTS MANAGER',
        };
      case 'remote':
        return {
          title: '远端运行时管理',
          description: '连接远程 SSH 主机集群，无缝部署、校验或热监控外部进程。',
          tag: 'REMOTE CLUSTER',
        };
      case 'events':
        return {
          title: '系统事件总线',
          description: '监听内核级 IPC 调用以及所有活动进程的消息 Payload 日志流。',
          tag: 'EVENT HUB',
        };
      case 'settings':
        return {
          title: '客户端系统设置',
          description: '管理 NapCatQQ-Desktop 底座的个性化偏好、网络代理与目录白名单。',
          tag: 'PREFERENCES',
        };
      default:
        return {
          title: '控制台',
          description: 'NapCatQQ 桌面助手。',
          tag: 'CONSOLE',
        };
    }
  };

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

  const info = getPageInfo();

  return (
    <div className="ndf-page-header">
      <div className="ndf-page-header-info">
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <Text size={500} weight="semibold" className="ndf-page-title">
            {info.title}
          </Text>
          <Badge size="small" appearance="outline" color="brand">
            {info.tag}
          </Badge>
        </div>
        <Text size={100} className="ndf-page-description">
          {info.description}
        </Text>
      </div>

      <div className="ndf-page-header-actions">
        <Button
          icon={isOpeningDir ? <Spinner size="tiny" /> : <FolderOpenRegular />}
          size="small"
          appearance="secondary"
          onClick={handleOpenDataDir}
          disabled={isOpeningDir}
        >
          打开缓存目录
        </Button>
      </div>
    </div>
  );
};
export default PageHeader;

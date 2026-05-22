import React from 'react';
import { Text, Badge } from '@fluentui/react-components';
import { DeviceEqRegular, GlobeRegular } from '@fluentui/react-icons';
import './StatusBar.css';

export const StatusBar: React.FC = () => {
  return (
    <footer className="ndf-statusbar">
      <div className="ndf-statusbar-left">
        <GlobeRegular className="ndf-status-icon success" />
        <Text size={100} className="ndf-status-text">
          运行时底座: <b>Tauri Core v2.0 - 正常就绪</b>
        </Text>
      </div>

      <div className="ndf-statusbar-right">
        <DeviceEqRegular className="ndf-status-icon" />
        <Text size={100} className="ndf-status-text">
          宿主架构: <b>x64 Native</b>
        </Text>
        <Badge appearance="filled" size="small" color="brand" style={{ marginLeft: '8px' }}>
          任务中心 (就绪)
        </Badge>
      </div>
    </footer>
  );
};
export default StatusBar;

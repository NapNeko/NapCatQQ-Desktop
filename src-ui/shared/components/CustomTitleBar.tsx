import React, { useEffect, useState } from 'react';
import { SubtractRegular, BoxRegular, DismissRegular, DeviceEqRegular } from '@fluentui/react-icons';
import { windowControlService } from '../../core/services/desktop.service';
import './CustomTitleBar.css';

interface CustomTitleBarProps {
  sidebarCollapsed: boolean;
}

export const CustomTitleBar: React.FC<CustomTitleBarProps> = ({ sidebarCollapsed }) => {
  const [isMaximized, setIsMaximized] = useState(false);

  const handleMinimize = () => {
    void windowControlService.minimize();
  };

  const handleMaximize = async () => {
    const next = await windowControlService.toggleMaximize();
    if (next !== null) {
      setIsMaximized(next);
    } else {
      // 浏览器预览：本地翻转一下让按钮看上去有反应。
      setIsMaximized((prev) => !prev);
    }
  };

  const handleClose = () => {
    void windowControlService.close();
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    (async () => {
      const initial = await windowControlService.isMaximized();
      if (!cancelled) setIsMaximized(initial);
      unlisten = await windowControlService.onResize((next) => {
        if (!cancelled) setIsMaximized(next);
      });
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  return (
    <header className={`ndf-titlebar ${sidebarCollapsed ? 'collapsed' : ''}`}>
      <div className="ndf-titlebar-left">
        <DeviceEqRegular className="ndf-titlebar-logo" />
        <span className="ndf-titlebar-title">NapCatQQ Desktop</span>
      </div>

      <div className="ndf-titlebar-drag-region" data-tauri-drag-region />

      <div className="ndf-titlebar-window-controls">
        <button type="button" className="ndf-win-btn" onClick={handleMinimize} title="最小化">
          <SubtractRegular />
        </button>
        <button type="button" className="ndf-win-btn" onClick={handleMaximize} title={isMaximized ? '还原' : '最大化'}>
          <BoxRegular />
        </button>
        <button type="button" className="ndf-win-btn ndf-win-close" onClick={handleClose} title="关闭">
          <DismissRegular />
        </button>
      </div>
    </header>
  );
};

export default CustomTitleBar;

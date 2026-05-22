import React, { useEffect, useState } from 'react';
import { SubtractRegular, BoxRegular, DismissRegular, DeviceEqRegular } from '@fluentui/react-icons';
import './CustomTitleBar.css';

interface CustomTitleBarProps {
  sidebarCollapsed: boolean;
}

export const CustomTitleBar: React.FC<CustomTitleBarProps> = ({ sidebarCollapsed }) => {
  const [isMaximized, setIsMaximized] = useState(false);

  const getTauriWindow = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      return getCurrentWindow();
    } catch {
      return null;
    }
  };

  const handleMinimize = async () => {
    const tauriWindow = await getTauriWindow();
    if (!tauriWindow) {
      console.log('浏览器预览: 最小化');
      return;
    }

    try {
      await tauriWindow.minimize();
    } catch (error) {
      console.error('窗口最小化失败:', error);
    }
  };

  const handleMaximize = async () => {
    const tauriWindow = await getTauriWindow();
    if (!tauriWindow) {
      console.log('浏览器预览: 最大化');
      setIsMaximized((current) => !current);
      return;
    }

    try {
      await tauriWindow.toggleMaximize();
      setIsMaximized(await tauriWindow.isMaximized());
    } catch (error) {
      console.error('窗口最大化/还原失败:', error);
    }
  };

  const handleClose = async () => {
    const tauriWindow = await getTauriWindow();
    if (!tauriWindow) {
      console.log('浏览器预览: 关闭');
      return;
    }

    try {
      await tauriWindow.close();
    } catch (error) {
      console.error('窗口关闭失败:', error);
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      const tauriWindow = await getTauriWindow();
      if (!tauriWindow) {
        return;
      }

      try {
        setIsMaximized(await tauriWindow.isMaximized());

        const { listen } = await import('@tauri-apps/api/event');
        const unlistenFn = await listen('tauri://resize', async () => {
          try {
            setIsMaximized(await tauriWindow.isMaximized());
          } catch (error) {
            console.error('刷新窗口最大化状态失败:', error);
          }
        });
        unlisten = unlistenFn;
      } catch (error) {
        console.error('初始化标题栏窗口状态失败:', error);
      }
    };

    void setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
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

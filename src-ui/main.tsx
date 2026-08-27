import React from 'react';
import ReactDOM from 'react-dom/client';
import { AppBootGate } from './app/AppBootGate';
import { AppProvidersNext } from './app/AppProvidersNext';

// 屏蔽 WebView/浏览器默认右键菜单（后退/刷新/审查），输入框除外（保留系统复制粘贴）
document.addEventListener('contextmenu', (e) => {
    const target = e.target as HTMLElement | null;
    const isInput =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;
    if (!isInput) {
        e.preventDefault();
    }
});

const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

const tree = (
    <AppProvidersNext>
        <AppBootGate />
    </AppProvidersNext>
);

// dev 下 StrictMode 会双挂载，放大 IPC 预热与动画初始化；仅生产启用。
if (import.meta.env.PROD) {
    root.render(<React.StrictMode>{tree}</React.StrictMode>);
} else {
    root.render(tree);
}
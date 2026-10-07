// 各窗口共用 React 挂载和原生右键菜单边界，不加载控制台业务。
import React from 'react';
import ReactDOM from 'react-dom/client';

document.addEventListener('contextmenu', (event) => {
    const target = event.target as HTMLElement | null;
    if (
        !(target instanceof HTMLInputElement) &&
        !(target instanceof HTMLTextAreaElement) &&
        !target?.isContentEditable
    ) {
        event.preventDefault();
    }
});

const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

export function renderRoot(tree: React.ReactElement) {
    // dev 双挂载会重复预热 IPC，保持现有启动语义。
    root.render(import.meta.env.PROD ? <React.StrictMode>{tree}</React.StrictMode> : tree);
}

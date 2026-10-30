// 各窗口共用 React 挂载和原生右键菜单边界，不加载控制台业务。
import React from 'react';
import ReactDOM from 'react-dom/client';
import { WindowFrame } from '../shared/components/next/WindowFrame';
import { isTauri, localOs } from '../core/domain/runtime/env';

if (isTauri && localOs() === 'linux') {
    document.documentElement.dataset.nativeOs = 'linux';
}

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
    const framed = <WindowFrame>{tree}</WindowFrame>;
    root.render(import.meta.env.PROD ? <React.StrictMode>{framed}</React.StrictMode> : framed);
}

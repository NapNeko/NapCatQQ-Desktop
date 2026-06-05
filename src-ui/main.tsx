import React from 'react';
import ReactDOM from 'react-dom/client';
import { AppBootGate } from './app/AppBootGate';
import { AppProvidersNext } from './app/AppProvidersNext';

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
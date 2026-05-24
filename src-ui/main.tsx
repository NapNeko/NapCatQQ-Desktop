import React from 'react';
import ReactDOM from 'react-dom/client';

// 蓝绿开关：VITE_UI_NEXT === '1' 走新 UI 树（Tailwind v4 + Radix），其它一律旧 Fluent 树。
// 旧版默认仍是入口，新版稳定后再翻转默认值，最后整层删除。
const useNextUi = import.meta.env.VITE_UI_NEXT === '1';

const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

if (useNextUi) {
  // 动态 import 防止旧 UI 也把 tailwind 全套拉进来。
  void Promise.all([
    import('./app/AppNext'),
    import('./app/AppProvidersNext'),
  ]).then(([{ AppNext }, { AppProvidersNext }]) => {
    root.render(
      <React.StrictMode>
        <AppProvidersNext>
          <AppNext />
        </AppProvidersNext>
      </React.StrictMode>,
    );
  });
} else {
  Promise.all([import('./app/App'), import('./app/providers/AppProviders')]).then(
    ([{ default: App }, { AppProviders }]) => {
      root.render(
        <React.StrictMode>
          <AppProviders>
            <App />
          </AppProviders>
        </React.StrictMode>,
      );
    },
  );
}

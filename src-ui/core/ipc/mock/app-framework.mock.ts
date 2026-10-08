// 浏览器预览模式下的应用端（Karin 等）假数据。
// 真 IPC 实装在 core/services/app-framework.service.ts。
// 实现按框架拆在同名目录 app-framework/ 下，这里只做原导出面的薄壳 re-export。

export { mockAppFrameworks } from './app-framework/manifests';
export { mockAppFrameworkApi } from './app-framework/index';
export { peekMockAppInstance } from './app-framework/state';

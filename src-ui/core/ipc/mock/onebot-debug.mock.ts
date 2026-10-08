// 调试台的浏览器预览假后端：三个 Bot、各自的通道、接口目录、能回数据的调用、会自己冒事件的事件流。
// 真 IPC 在 `core/services/onebot-debug.service.ts`。
//
// 全部状态只在内存里，刷新页面即重置；随机数都有固定种子，同样的操作序列得到同样的数据。
// 实现按处理域拆在 ./onebot-debug/ 分模块里（api / bots / state / handlers / call / collections），
// 本文件只保留原导出面。
export { onebotDebugMock, resetOnebotDebugMock } from './onebot-debug/api';

// 调试台浏览器预览用的接口目录：手写 32 个动作（每个后端 31 个），覆盖全部分类和三档安全等级。
// 其中 `debug_huge_response` 是预览专用的假动作，用来走查超大回包被截断的界面。
//
// 定义按接口域拆在 ./onebot-debug/catalog-*.ts 里，本文件只保留原导出面。
export {
    buildMockSpec,
    buildMockCatalog,
    mockRequiredParams,
    type MockSpecOptions,
} from './onebot-debug/catalog-build';

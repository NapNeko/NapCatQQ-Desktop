// 应用端模块给别的模块用的入口：组件页按主机新建、导入实例用的就是这两个对话框，
// 别的模块只从这里拿，不伸进应用端的内部文件。
// 只挂别处真要静态引入的东西，别把页面、详情 Tab 挂进来：引了这个入口的页面会把它们的依赖一起打进自己的包。

export { CreateInstanceDialog, type CreateInstanceRequest } from './CreateInstanceDialog';
export { ImportInstanceDialog, type ImportInstanceTarget } from './ImportInstanceDialog';

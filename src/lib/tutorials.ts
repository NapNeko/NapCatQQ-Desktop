export type Tutorial = {
  id: string;
  title: string;
  environment: string;
  poster: string;
  steps: string[];
  summary: string;
};

export const tutorials: Tutorial[] = [
  { id: 'first-bot', title: '本机 · NapCat', environment: '组件已就绪，不会登录 QQ', poster: 'bot-create', steps: ['列表', '新建', '填写', '保存', '启动', '日志'], summary: '从新建 Bot 到查看日志' },
  { id: 'connections', title: 'OneBot · HTTP', environment: '只保存演示配置，不会监听端口', poster: 'bot-connections', steps: ['配置', '连接', '新增', '填写', '保存', '核对'], summary: '给 Bot 添加一个 HTTP 服务' },
  { id: 'bots', title: 'Bot · 日常管理', environment: 'Bot 状态为模拟，不会启动 QQ', poster: 'bots', steps: ['状态', '停止', '配置', '修改', '保存', '启动'], summary: '停止、修改配置，再重新启动' },
  { id: 'tasks-logs', title: 'Bot · 运行日志', environment: '内置样例日志，不连接真实进程', poster: 'bot-logs', steps: ['日志', '上下文', '搜索', '等级', '暂停', '恢复'], summary: '从日志里找到需要检查的那一行' },
  { id: 'components', title: '组件 · 本机', environment: '模拟安装进度，不会下载或安装软件', poster: 'components', steps: ['主机', '缺失项', 'QQ', '运行库', '协议端', '核对'], summary: '在本机准备 QQ 与 NapCat' },
  { id: 'settings', title: '设置 · 外观与功能', environment: '设置只在这次演示里生效', poster: 'settings-appearance', steps: ['外观', '圆角', '功能', '开启', '保存', '核对'], summary: '调整外观，整理常用入口' },
  { id: 'network', title: '设置 · 网络', environment: '连接测试为模拟，不会发起代理请求', poster: 'settings-network', steps: ['网络', '代理', '线路', '测试', '保存', '核对'], summary: '设置代理与下载线路' },
  { id: 'backup', title: '数据 · 下载缓存', environment: '扫描与清理只作用于样例数据', poster: 'settings-data', steps: ['数据', '范围', '扫描', '选择', '清理', '结果'], summary: '先看清缓存内容，再选择清理' },
  { id: 'overview', title: '概览 · 运行状态', environment: '实例与指标均为演示数据', poster: 'overview', steps: ['概览', '状态', '通知', '版本', '定位', '日志'], summary: '从概览找到需要处理的 Bot' },
  { id: 'apps', title: '应用端 · 对接', environment: '应用已准备好，对接只写入演示数据', poster: 'app-link', steps: ['实例', '详情', '设置', '对接', '计划', '确认'], summary: '把一个应用实例接到协议 Bot' },
  { id: 'debugging', title: '调试台 · OneBot', environment: '接口返回为样例，不访问 QQ', poster: 'debug', steps: ['账号', '搜索', '参数', '调用', '历史', '收藏'], summary: '调用一次 get_login_info，查看返回值' },
  { id: 'chat', title: '聊天 · 消息操作', environment: '会话与消息为样例，不会发送到 QQ', poster: 'chat', steps: ['账号', '会话', '引用', '发送', '搜索', '定位'], summary: '回复一条演示消息，再搜索定位' },
];

export function tutorialForDoc(id: string) {
  return tutorials.find(tutorial => tutorial.id === id);
}

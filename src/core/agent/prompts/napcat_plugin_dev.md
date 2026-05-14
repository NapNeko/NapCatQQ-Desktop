# NapCat 插件开发知识库

你是 NapCat 插件开发助手，具备完整的 NapCat 插件开发知识。以下是你需要掌握的核心知识。

## 1. 插件项目结构

NapCat 插件遵循以下标准项目结构：

```
my-plugin/
├── src/
│   ├── index.ts              # 入口文件，导出生命周期函数
│   ├── core/
│   │   └── state.ts          # 全局单例状态管理
│   ├── handlers/             # 消息处理逻辑
│   │   └── message.ts
│   ├── services/             # API 路由注册
│   │   └── api.ts
│   └── webui/                # React 前端（可选）
│       ├── App.tsx
│       └── index.tsx
├── package.json
├── tsconfig.json
└── vite.config.ts
```

## 2. TypeScript/ESM 模块规范

NapCat 插件使用 TypeScript 编写，遵循 ESM 模块规范：

```typescript
// package.json 中必须声明
{
  "type": "module",
  "main": "dist/index.js"
}

// tsconfig.json 关键配置
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "dist"
  }
}
```

导入规范：
- 使用 `import`/`export` 语法，不使用 `require`
- 类型导入使用 `import type { ... } from '...'`
- 从 `napcat-types` 包导入 NapCat 相关类型

```typescript
import type { NapCatPluginContext, OneBotAction } from 'napcat-types';
```

## 3. 插件生命周期函数

插件通过导出以下生命周期函数与 NapCat 交互：

### plugin_init(ctx: NapCatPluginContext): Promise<void>

插件初始化函数，在插件加载时调用。用于注册路由、初始化状态、设置定时任务等。

```typescript
export async function plugin_init(ctx: NapCatPluginContext): Promise<void> {
  ctx.logger.info('插件初始化');
  // 注册 WebUI 路由
  ctx.router.get('/api/my-plugin/status', (req, res) => {
    res.json({ status: 'ok' });
  });
}
```

### plugin_onmessage(ctx: NapCatPluginContext, message: object): Promise<void>

消息处理函数，当收到消息时调用。

```typescript
export async function plugin_onmessage(
  ctx: NapCatPluginContext,
  message: object
): Promise<void> {
  // 处理收到的消息
  const msg = message as any;
  if (msg.raw_message?.startsWith('/hello')) {
    await ctx.actions.call(
      'send_msg',
      { message_type: msg.message_type, user_id: msg.user_id, group_id: msg.group_id },
      [{ type: 'text', data: { text: 'Hello!' } }],
      null
    );
  }
}
```

### plugin_onevent(ctx: NapCatPluginContext, event: object): Promise<void>

事件处理函数，当收到非消息事件（如群成员变动、好友请求等）时调用。

```typescript
export async function plugin_onevent(
  ctx: NapCatPluginContext,
  event: object
): Promise<void> {
  const evt = event as any;
  if (evt.post_type === 'notice' && evt.notice_type === 'group_increase') {
    ctx.logger.info(`新成员加入群 ${evt.group_id}: ${evt.user_id}`);
  }
}
```

### plugin_cleanup(ctx: NapCatPluginContext): Promise<void>

插件清理函数，在插件卸载或 NapCat 关闭时调用。用于释放资源、保存状态。

```typescript
export async function plugin_cleanup(ctx: NapCatPluginContext): Promise<void> {
  ctx.logger.info('插件清理');
  // 保存状态、关闭连接等
}
```

### plugin_config_ui(ctx: NapCatPluginContext): NapCatConfig

配置 UI 定义函数，返回插件的配置 Schema，用于在 WebUI 中渲染配置界面。

```typescript
export function plugin_config_ui(ctx: NapCatPluginContext): NapCatConfig {
  return ctx.NapCatConfig
    .text('api_key', 'API Key', '请输入你的 API Key')
    .number('max_retry', '最大重试次数', 3)
    .boolean('enable_log', '启用日志', true)
    .select('mode', '运行模式', 'normal', [
      { label: '普通模式', value: 'normal' },
      { label: '调试模式', value: 'debug' },
    ]);
}
```

### plugin_on_config_change(ctx: NapCatPluginContext, config: object): Promise<void>

配置变更回调，当用户在 WebUI 中修改配置后调用。

```typescript
export async function plugin_on_config_change(
  ctx: NapCatPluginContext,
  config: object
): Promise<void> {
  ctx.logger.info('配置已更新', config);
  // 应用新配置
}
```

## 4. NapCatPluginContext API

`ctx` 对象是插件与 NapCat 交互的核心接口：

### ctx.actions

OneBot11 Action 调用接口。通过 `ctx.actions.call()` 方法调用 OneBot11 标准 API。

```typescript
// call(action: string, params: object, message?: MessageSegment[], extra?: any)
// 4 个参数：action 名称、参数对象、消息段数组（可选）、额外参数（可选）

// 发送私聊消息
await ctx.actions.call(
  'send_msg',
  { message_type: 'private', user_id: 123456 },
  [{ type: 'text', data: { text: '你好' } }],
  null
);

// 发送群消息
await ctx.actions.call(
  'send_msg',
  { message_type: 'group', group_id: 789012 },
  [{ type: 'text', data: { text: '群公告' } }],
  null
);

// 获取群成员列表
const members = await ctx.actions.call(
  'get_group_member_list',
  { group_id: 789012 },
  null,
  null
);

// 撤回消息
await ctx.actions.call(
  'delete_msg',
  { message_id: 12345 },
  null,
  null
);
```

### ctx.logger

日志记录器，支持多级别日志输出。

```typescript
ctx.logger.info('信息日志');
ctx.logger.warn('警告日志');
ctx.logger.error('错误日志');
ctx.logger.debug('调试日志');
```

### ctx.router

Express 风格的路由注册器，用于在 NapCat WebUI 中注册自定义 API 和页面。

```typescript
// 注册 GET 路由
ctx.router.get('/api/my-plugin/data', (req, res) => {
  res.json({ data: [] });
});

// 注册 POST 路由
ctx.router.post('/api/my-plugin/action', (req, res) => {
  const body = req.body;
  res.json({ success: true });
});

// 注册静态文件目录
ctx.router.static('/my-plugin/assets', './dist/assets');

// 注册页面路由（用于 WebUI 中嵌入插件页面）
ctx.router.page('/my-plugin', './dist/index.html');
```

### ctx.dataPath

插件数据存储目录路径（字符串）。用于持久化插件运行时数据。

```typescript
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';

// 确保数据目录存在
if (!existsSync(ctx.dataPath)) {
  mkdirSync(ctx.dataPath, { recursive: true });
}

// 读写数据文件
const dataFile = join(ctx.dataPath, 'state.json');
writeFileSync(dataFile, JSON.stringify(state), 'utf-8');
const loaded = JSON.parse(readFileSync(dataFile, 'utf-8'));
```

### ctx.configPath

插件配置文件路径（字符串）。NapCat 自动管理配置的读写。

### ctx.NapCatConfig

配置 Schema 构建器，用于定义插件配置界面。支持以下方法：

```typescript
ctx.NapCatConfig
  .text(key, label, defaultValue?)          // 文本输入
  .number(key, label, defaultValue?)        // 数字输入
  .boolean(key, label, defaultValue?)       // 布尔开关
  .select(key, label, defaultValue, options)  // 单选下拉
  .multiSelect(key, label, defaultValue, options)  // 多选
  .html(key, htmlContent)                   // 自定义 HTML 展示
  .plainText(key, text)                     // 纯文本说明
  .combine(key, label, configs)             // 组合配置组
```

### ctx.pluginManager

插件管理器接口，可查询其他已加载插件的信息。

```typescript
// 获取已加载的插件列表
const plugins = ctx.pluginManager.getLoadedPlugins();

// 检查某个插件是否已加载
const isLoaded = ctx.pluginManager.isPluginLoaded('other-plugin-id');
```

### ctx.getPluginExports(pluginId: string)

获取其他插件导出的接口，用于插件间通信。

```typescript
// 获取另一个插件的导出
const otherPlugin = ctx.getPluginExports('other-plugin-id');
if (otherPlugin) {
  await otherPlugin.someMethod();
}
```

## 5. OneBot11 Action 调用规范

所有 OneBot11 Action 通过 `ctx.actions.call()` 调用，固定 4 个参数：

```typescript
ctx.actions.call(action, params, message, extra)
```

| 参数 | 类型 | 说明 |
|------|------|------|
| action | string | OneBot11 API 名称 |
| params | object | API 参数对象 |
| message | MessageSegment[] \| null | 消息段数组（发送消息时使用） |
| extra | any \| null | 额外参数（通常为 null） |

常用 Action 列表：

| Action | 说明 | 关键参数 |
|--------|------|----------|
| send_msg | 发送消息 | message_type, user_id/group_id |
| delete_msg | 撤回消息 | message_id |
| get_msg | 获取消息 | message_id |
| get_group_list | 获取群列表 | - |
| get_group_member_list | 获取群成员列表 | group_id |
| get_group_member_info | 获取群成员信息 | group_id, user_id |
| set_group_kick | 踢出群成员 | group_id, user_id |
| set_group_ban | 禁言 | group_id, user_id, duration |
| set_group_card | 设置群名片 | group_id, user_id, card |
| get_stranger_info | 获取陌生人信息 | user_id |
| get_friend_list | 获取好友列表 | - |
| set_friend_add_request | 处理好友请求 | flag, approve |
| set_group_add_request | 处理加群请求 | flag, approve |

消息段（MessageSegment）类型：

```typescript
// 文本
{ type: 'text', data: { text: '内容' } }

// 图片
{ type: 'image', data: { file: 'https://example.com/img.png' } }

// @某人
{ type: 'at', data: { qq: '123456' } }

// 表情
{ type: 'face', data: { id: '1' } }

// 回复
{ type: 'reply', data: { id: '消息ID' } }

// JSON 卡片
{ type: 'json', data: { data: JSON.stringify(cardData) } }
```

## 6. Vite 构建系统

NapCat 插件使用 Vite 作为构建工具：

```typescript
// vite.config.ts
import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      formats: ['es'],
      fileName: 'index',
    },
    rollupOptions: {
      external: ['napcat-types', 'fs', 'path', 'os'],
    },
    outDir: 'dist',
    emptyDirBeforeWrite: true,
  },
});
```

构建命令：
- `npm run build` — 构建插件
- `npm run dev` — 开发模式（watch）

如果插件包含 WebUI 前端，需要额外的 Vite 配置来构建 React 应用：

```typescript
// vite.config.webui.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist/webui',
  },
});
```

## 7. WebUI 路由注册模式

在 `plugin_init` 中注册路由：

```typescript
export async function plugin_init(ctx: NapCatPluginContext): Promise<void> {
  // REST API 路由
  ctx.router.get('/api/my-plugin/status', (req, res) => {
    res.json({ online: true, version: '1.0.0' });
  });

  ctx.router.post('/api/my-plugin/config', (req, res) => {
    const newConfig = req.body;
    // 保存配置...
    res.json({ success: true });
  });

  // 静态资源
  ctx.router.static('/my-plugin/assets', join(__dirname, '../dist/webui/assets'));

  // 页面路由（嵌入 WebUI）
  ctx.router.page('/my-plugin', join(__dirname, '../dist/webui/index.html'));
}
```

## 8. NapCat 插件配置系统

使用 `ctx.NapCatConfig` 构建配置 Schema：

```typescript
export function plugin_config_ui(ctx: NapCatPluginContext) {
  return ctx.NapCatConfig
    .plainText('info', '插件说明：这是一个示例插件')
    .text('webhook_url', 'Webhook URL', 'https://example.com/hook')
    .number('interval', '检查间隔（秒）', 60)
    .boolean('auto_reply', '自动回复', false)
    .select('language', '语言', 'zh', [
      { label: '中文', value: 'zh' },
      { label: 'English', value: 'en' },
    ])
    .multiSelect('features', '启用功能', ['basic'], [
      { label: '基础功能', value: 'basic' },
      { label: '高级功能', value: 'advanced' },
      { label: '实验功能', value: 'experimental' },
    ])
    .combine('advanced_settings', '高级设置', (cfg) =>
      cfg
        .number('timeout', '超时时间', 30)
        .boolean('debug', '调试模式', false)
    );
}
```

## 9. 数据持久化模式

使用 `ctx.dataPath` 进行数据持久化：

```typescript
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';

// 状态管理示例
class PluginState {
  private filePath: string;
  private data: Record<string, any> = {};

  constructor(dataPath: string) {
    if (!existsSync(dataPath)) {
      mkdirSync(dataPath, { recursive: true });
    }
    this.filePath = join(dataPath, 'data.json');
    this.load();
  }

  private load(): void {
    if (existsSync(this.filePath)) {
      this.data = JSON.parse(readFileSync(this.filePath, 'utf-8'));
    }
  }

  save(): void {
    writeFileSync(this.filePath, JSON.stringify(this.data, null, 2), 'utf-8');
  }

  get(key: string): any {
    return this.data[key];
  }

  set(key: string, value: any): void {
    this.data[key] = value;
    this.save();
  }
}
```

## 10. 完整插件示例

```typescript
// src/index.ts
import type { NapCatPluginContext } from 'napcat-types';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';

let state: { count: number } = { count: 0 };
let dataFile: string;

export async function plugin_init(ctx: NapCatPluginContext): Promise<void> {
  ctx.logger.info('示例插件初始化');

  // 初始化数据目录
  if (!existsSync(ctx.dataPath)) {
    mkdirSync(ctx.dataPath, { recursive: true });
  }
  dataFile = join(ctx.dataPath, 'state.json');

  // 加载持久化状态
  if (existsSync(dataFile)) {
    state = JSON.parse(readFileSync(dataFile, 'utf-8'));
  }

  // 注册 API 路由
  ctx.router.get('/api/example/count', (req, res) => {
    res.json({ count: state.count });
  });
}

export async function plugin_onmessage(
  ctx: NapCatPluginContext,
  message: object
): Promise<void> {
  const msg = message as any;
  if (msg.raw_message === '/count') {
    state.count++;
    writeFileSync(dataFile, JSON.stringify(state), 'utf-8');

    await ctx.actions.call(
      'send_msg',
      { message_type: msg.message_type, user_id: msg.user_id, group_id: msg.group_id },
      [{ type: 'text', data: { text: `当前计数: ${state.count}` } }],
      null
    );
  }
}

export async function plugin_onevent(
  ctx: NapCatPluginContext,
  event: object
): Promise<void> {
  // 处理非消息事件
}

export async function plugin_cleanup(ctx: NapCatPluginContext): Promise<void> {
  writeFileSync(dataFile, JSON.stringify(state), 'utf-8');
  ctx.logger.info('示例插件已清理');
}

export function plugin_config_ui(ctx: NapCatPluginContext) {
  return ctx.NapCatConfig
    .plainText('desc', '这是一个计数器示例插件')
    .number('initial_count', '初始计数', 0)
    .boolean('auto_save', '自动保存', true);
}

export async function plugin_on_config_change(
  ctx: NapCatPluginContext,
  config: object
): Promise<void> {
  ctx.logger.info('配置已更新', config);
}
```

// 主窗口路由类型。导航、页面和侧栏直接依赖此定义，彼此不依赖表现组件。

export type AppRoute =
    | 'overview'
    | 'bots'
    | 'chat'
    | 'apps'
    | 'debug'
    | 'components'
    | 'docker'
    | 'remote'
    | 'tasks'
    | 'settings';

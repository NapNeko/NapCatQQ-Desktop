// 主窗口路由枚举：定义权威在 core/domain，Sidebar re-export 供历史 import 路径兼容。

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

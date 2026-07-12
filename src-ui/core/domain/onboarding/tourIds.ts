// Spotlight 锚点 id。DOM 上 data-tour-id 与步骤 target 必须同一字面量。

export const TOUR_IDS = {
    navComponents: 'nav-components',
    navBots: 'nav-bots',
    hostSwitcher: 'host-switcher',
    hostTabDemoRemote: 'host-tab-demo-remote',
    groupFramework: 'comp-group-framework',
    groupRuntime: 'comp-group-runtime',
    rowNapcat: 'comp-row-napcat',
    rowSnowluma: 'comp-row-snowluma',
    rowNodejs: 'comp-row-nodejs',
    rowQq: 'comp-row-qq',
    rowNovnc: 'comp-row-novnc',
    rowDocker: 'comp-row-docker',
    /** 机器人列表页标题区 */
    botListHeader: 'bot-list-header',
    /** 机器人列表右下角「新增 Bot」FAB */
    botCreateFab: 'bot-create-fab',
    /** 空列表时的「创建第一个实例」 */
    botCreateEmpty: 'bot-create-empty',
    /** 新建/编辑配置页顶栏 */
    botConfigHeader: 'bot-config-header',
    /** 身份 Tab · 账号身份区块 */
    botIdentitySection: 'bot-identity-section',
    /** 身份 Tab · 运行场景（本机/远程） */
    botRuntimeSection: 'bot-runtime-section',
    /** 连接 Tab 触发器 */
    botConnectionsTab: 'bot-connections-tab',
    /** 连接 Tab 主体 */
    botConnectionsBody: 'bot-connections-body',
    /** 右上角保存条 */
    botSaveActions: 'bot-save-actions',
} as const;

export type TourTargetId = (typeof TOUR_IDS)[keyof typeof TOUR_IDS];

/** 演示远端主机 id：不进 servers.json，仅 tour / 组件页注入。 */
export const DEMO_REMOTE_HOST_ID = 'tour:demo-remote';

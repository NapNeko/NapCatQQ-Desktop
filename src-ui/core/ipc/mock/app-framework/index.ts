// 聚合入口：把各框架的假 API 片段拼成与拆分前同名的 mockAppFrameworkApi。
// 键序与原来不同，但调用方只按键取值，行为不变。
import { instanceApi } from './instance-api';
import { linkApi } from './link-api';
import { pluginApi } from './plugin-api';
import { neobotPanelApi } from './neobot-panel';
import { mockAstrBotApi } from './astrbot';
import { mockKoishiApi } from './koishi';
import { mockMaiBotApi } from './maibot';

export const mockAppFrameworkApi = {
    ...instanceApi,
    ...neobotPanelApi,
    ...linkApi,
    ...pluginApi,
    ...mockAstrBotApi,
    ...mockKoishiApi,
    ...mockMaiBotApi,
};

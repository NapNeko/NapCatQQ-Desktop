import { TabsContent } from '../../../../shared/ui';
import { NeoBotConsoleTab } from './NeoBotConsoleTab';
import type { FrameworkDetailProps, FrameworkNavGroup, FrameworkUiModule } from '../frameworkUi';

// 目前只有「Web 控制台」一页。连接、模型、提示词、记忆、插件这些页签等桌面端真正接上
// 面板 API 之后再逐步加（见 NeoBotConsoleTab 里的能力地图）。原始文件、日志由外壳追加。
const NAV: readonly FrameworkNavGroup[] = [
    { id: 'console', items: [{ value: 'console', label: 'Web 控制台' }] },
];

const FILL_PANE = new Set(['console']);

function NeoBotFrameworkDetail({ instance, onOpenWebUi }: FrameworkDetailProps) {
    return (
        <TabsContent value="console" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
            <NeoBotConsoleTab instance={instance} onOpenWebUi={onOpenWebUi} />
        </TabsContent>
    );
}

export const neobotFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'console',
    // 这页只读 + 一个跳转按钮，没有可保存的类型化配置
    typedTabs: new Set(),
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'console',
    Detail: NeoBotFrameworkDetail,
};

import { TabsContent } from '../../../../shared/ui';
import { NeoBotConsoleTab } from './NeoBotConsoleTab';
import { NeoBotOverviewTab } from './NeoBotOverviewTab';
import type { FrameworkDetailProps, FrameworkNavGroup, FrameworkUiModule } from '../frameworkUi';

// 概览：面板首页那几个数。控制台：面板能力地图 + 面板凭据 + 打开入口。
// 模型 / 提示词 / 记忆 / 插件等页签沿用同一条数据通路（panelCall → 各自的 /api/*），
// 一个页签接一个端点，接一个加一个。原始文件、日志由外壳追加。
const NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'overview', label: '概览' }] },
    { id: 'console', items: [{ value: 'console', label: 'Web 控制台' }] },
];

const FILL_PANE = new Set(['overview', 'console']);

function NeoBotFrameworkDetail({ instance, onOpenWebUi }: FrameworkDetailProps) {
    return (
        <>
            <TabsContent value="overview" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotOverviewTab instanceId={instance.id} />
            </TabsContent>
            <TabsContent value="console" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NeoBotConsoleTab instance={instance} onOpenWebUi={onOpenWebUi} />
            </TabsContent>
        </>
    );
}

export const neobotFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'overview',
    // 两页都不挂保存条：概览只读，控制台只有凭据一个输入（自己存）
    typedTabs: new Set(),
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'overview',
    Detail: NeoBotFrameworkDetail,
};
import { TabsContent } from '../../../../shared/ui';
import { NeoBotConsoleTab } from './NeoBotConsoleTab';
import { NeoBotDeployTab } from './NeoBotDeployTab';
import { NeoBotMemoryTab } from './NeoBotMemoryTab';
import { NeoBotModelsTab } from './NeoBotModelsTab';
import { NeoBotOverviewTab } from './NeoBotOverviewTab';
import { NeoBotPluginsTab } from './NeoBotPluginsTab';
import { NeoBotPromptsTab } from './NeoBotPromptsTab';
import type { FrameworkDetailProps, FrameworkNavGroup, FrameworkUiModule } from '../frameworkUi';

// 「部署」放第一位并作默认页：新实例第一件该做的事是把它跑起来、连上 QQ，而不是看统计。
// 其余页都是只读的面板数据（一个页签一个端点，状态处理见 PanelStateView）。
// 原始文件、日志由外壳追加。
const NAV: readonly FrameworkNavGroup[] = [
    {
        id: 'home',
        items: [
            { value: 'deploy', label: '部署' },
            { value: 'overview', label: '概览' },
        ],
    },
    {
        id: 'ai',
        label: 'AI',
        items: [
            { value: 'models', label: '模型' },
            { value: 'prompts', label: '提示词' },
            { value: 'memory', label: '记忆' },
        ],
    },
    { id: 'extend', label: '扩展', items: [{ value: 'plugins', label: '插件' }] },
    { id: 'console', items: [{ value: 'console', label: 'Web 控制台' }] },
];

// 这些页都是**普通堆叠内容**（不是靠内层自己滚的网格/文件树），所以交给外壳滚：
// 声明成 fillPane 会拿到 overflow-hidden，而页面自己不滚，长内容就被裁掉——实测「提示词」页
// 滑不动就是这个原因。对照 astrbot：它只把 plugins（自己有滚动区的网格）标成 fillPane。
const FILL_PANE = new Set<string>();

const PANE = 'flex min-h-0 flex-1 flex-col pt-2';

function NeoBotFrameworkDetail({
    instance,
    onOpenWebUi,
    onGoTab,
    onNavigate,
}: FrameworkDetailProps) {
    const instanceId = instance.id;
    return (
        <>
            <TabsContent value="deploy" className={PANE}>
                <NeoBotDeployTab instance={instance} onGoTab={onGoTab} onNavigate={onNavigate} />
            </TabsContent>
            <TabsContent value="overview" className={PANE}>
                <NeoBotOverviewTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="models" className={PANE}>
                <NeoBotModelsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="prompts" className={PANE}>
                <NeoBotPromptsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="memory" className={PANE}>
                <NeoBotMemoryTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="plugins" className={PANE}>
                <NeoBotPluginsTab instanceId={instanceId} onGoTab={onGoTab} />
            </TabsContent>
            <TabsContent value="console" className={PANE}>
                <NeoBotConsoleTab instance={instance} onOpenWebUi={onOpenWebUi} />
            </TabsContent>
        </>
    );
}

export const neobotFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'deploy',
    // 都不挂保存条：各页只读，或自己把动作 POST 给面板
    typedTabs: new Set(),
    fillPaneTabs: FILL_PANE,
    tabForIssue: () => 'deploy',
    Detail: NeoBotFrameworkDetail,
};

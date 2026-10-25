// 浏览器专用 NeoBot 预览，所有面板操作只使用现有模拟数据。
import { useEffect, useState, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { neobotFrameworkUi } from '../modules/apps/detail/neobot/neobotFrameworkUi';
import { DetailSideNav } from '../modules/apps/detail/DetailSideNav';
import { instances } from '../core/ipc/mock/app-framework/state';
import { appFrameworkService } from '../core/services/app-framework.service';
import { globalInfoBarStore } from '../hooks/ui/globalInfoBarStore';
import { Badge, Button, InfoBarStack, Select, Tabs, TooltipProvider } from '../shared/ui';
import '../app/index.css';

const params = new URLSearchParams(location.search);
const scenario = params.get('scenario') ?? 'ready';
const instance = { ...instances.find((item) => item.framework_id === 'neobot')! };
if (scenario === 'legacy') instance.installed_version = '1.2.3';
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
const originalCall = appFrameworkService.panelCall;
const originalSave = appFrameworkService.setPanelPassword;
let unlocked = scenario === 'ready' || scenario === 'partial' || scenario === 'rejected';
let passwordRejected = scenario === 'rejected';

if (import.meta.env.DEV) {
    appFrameworkService.panelPasswordSet = async () => unlocked;
    appFrameworkService.setPanelPassword = async (id, password) => {
        await originalSave(id, password);
        unlocked = !!password.trim();
        passwordRejected = false;
    };
    appFrameworkService.panelCall = async (id, method, path, body) => {
        if (scenario === 'offline') return { kind: 'unreachable', message: '模拟：实例未启动' };
        if (path === '/api/auth/status') {
            return { kind: 'ok', data: { configured: scenario !== 'setup', setup_allowed: true } };
        }
        if (scenario === 'partial' && path === '/api/system') {
            return { kind: 'failed', message: '模拟：系统信息暂时不可用' };
        }
        if (
            path !== '/api/auth/setup' &&
            (scenario === 'legacy' || passwordRejected || !unlocked)
        ) {
            return { kind: 'unauthorized', message: '模拟：需要登录面板' };
        }
        return originalCall(id, method, path, body);
    };
}

export function Preview() {
    const [tab, setTab] = useState(params.get('tab') ?? 'runtime');
    const [dark, setDark] = useState(params.get('theme') === 'dark');
    const { bars } = useSyncExternalStore(
        globalInfoBarStore.subscribe,
        globalInfoBarStore.getSnapshot,
    );
    const Detail = neobotFrameworkUi.Detail;
    const title = neobotFrameworkUi.nav
        .flatMap((group) => group.items)
        .find((item) => item.value === tab)?.label;
    const goTab = (next: string) => {
        setTab(next);
        const url = new URL(location.href);
        url.searchParams.set('tab', next);
        history.replaceState(null, '', url);
    };
    const openConsole = () =>
        globalInfoBarStore.push({
            tone: 'info',
            title: '浏览器预览',
            content: '此入口仅展示模拟数据，不连接实际 NeoBot。',
        });
    useEffect(() => {
        document.documentElement.dataset.theme = dark ? 'dark' : 'light';
        document.documentElement.dataset.themeScheme = dark ? 'dark' : 'light';
    }, [dark]);
    return (
        <QueryClientProvider client={client}>
            <TooltipProvider>
                <main className="flex h-screen min-w-0 flex-col bg-canvas p-4 sm:p-6">
                    <header className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border-subtle pb-4">
                        <div>
                            <div className="flex items-center gap-2">
                                <h1 className="text-md font-semibold text-text">NeoBot</h1>
                                <Badge tone="neutral">界面预览</Badge>
                            </div>
                            <p className="mt-1 text-xs text-text-tertiary">
                                模拟数据，不连接实际实例
                            </p>
                        </div>
                        <div className="flex items-center gap-2">
                            <Select
                                aria-label="预览场景"
                                value={scenario}
                                items={[
                                    { value: 'ready', label: '正常数据' },
                                    { value: 'unauthorized', label: '未保存密码' },
                                    { value: 'rejected', label: '密码失效' },
                                    { value: 'offline', label: '面板离线' },
                                    { value: 'partial', label: '部分读取失败' },
                                    { value: 'legacy', label: '旧版本' },
                                    { value: 'setup', label: '首次设置' },
                                ]}
                                onValueChange={(value) => {
                                    const url = new URL(location.href);
                                    url.searchParams.set('scenario', value);
                                    location.assign(url);
                                }}
                            />
                            <Button size="sm" variant="secondary" onClick={() => setDark(!dark)}>
                                {dark ? '浅色' : '深色'}
                            </Button>
                        </div>
                    </header>
                    <Tabs
                        value={tab}
                        onValueChange={goTab}
                        orientation="vertical"
                        className="flex min-h-0 flex-1"
                    >
                        <DetailSideNav groups={neobotFrameworkUi.nav} badges={{}} />
                        <div className="min-w-0 flex-1 overflow-y-auto px-4 pb-6 sm:px-6">
                            <h2 className="pt-5 text-sm font-semibold text-text">{title}</h2>
                            <Detail
                                instance={instance}
                                onSaveHandle={() => {}}
                                onGoTab={goTab}
                                onOpenLink={openConsole}
                                onOpenWebUi={openConsole}
                                onNavBadges={() => {}}
                            />
                        </div>
                    </Tabs>
                </main>
                <InfoBarStack items={bars} onDismiss={globalInfoBarStore.dismiss} />
            </TooltipProvider>
        </QueryClientProvider>
    );
}

if (import.meta.env.DEV) createRoot(document.getElementById('root')!).render(<Preview />);

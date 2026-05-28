// 客户端偏好页（参考 shadcn/ui Settings Recipe）。
//
// 视觉骨架：
//   - 页头：紧凑标题 + 一句副标
//   - Tabs 横向分类：通用 / 数据 / 关于
//   - 每个 Tab 内用 'space-y-6'：每行 flex items-center justify-between
//     左侧 Label + Description 堆叠，右侧控件
//   - 行间用 hairline 短线分隔（border-b border-border-subtle pb-6）
//   - 容器宽度由 AppNext 的 main 控制（xl:max-w-[1280px]），本页不再自行限宽，
//     避免设置页跟其它页宽度不一致
//
// 这版跟前几版不同：不画大卡片，不画 Card padding-lg 容器；行布局
// space-between 让控件天然右对齐，比 SettingRow 抽象更直接。

import { useState } from 'react';
import { Sun, Moon, MonitorCog } from 'lucide-react';
import { Button, Select, Switch, Tabs, TabsContent, TabsList, TabsTrigger } from '../../shared/ui';
import {
    preferencesStore,
    usePreferences,
    type ThemeMode,
} from '../../hooks/preferences/preferencesStore';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';

export function SettingsPageNext() {
    const prefs = usePreferences();
    const { bootstrap, openDataDir, isOpeningDir } = useBootstrap();
    const [tab, setTab] = useState('general');

    const dataRoot = (bootstrap as any)?.data_root ?? '—';
    const version = (bootstrap as any)?.app_version ?? '0.1.0-alpha.1';

    const handleOpen = async () => {
        try {
            await openDataDir();
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('打开数据目录失败:', err);
        }
    };

    return (
        <div className="flex h-full min-h-0 flex-col">
            <header className="shrink-0 pb-4 pt-2">
                <h1 className="font-display text-xl font-semibold leading-none text-text">
                    设置
                </h1>
                <p className="mt-1.5 text-[13px] text-text-secondary">
                    客户端偏好与系统环境
                </p>
            </header>

            <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                <Tabs value={tab} onValueChange={setTab} className="w-full">
                    <TabsList className="mb-6">
                        <TabsTrigger value="general">通用</TabsTrigger>
                        <TabsTrigger value="data">数据</TabsTrigger>
                        <TabsTrigger value="about">关于</TabsTrigger>
                    </TabsList>

                    <TabsContent value="general" className="space-y-6">
                        <FieldRow
                            label="主题"
                            description="切换后立即生效，无需重启"
                        >
                            <ThemeSegment
                                value={prefs.theme}
                                onChange={preferencesStore.setTheme}
                            />
                        </FieldRow>

                        <FieldRow
                            label="主页吉祥物"
                            description="概览页右上角的猫娘"
                        >
                            <Switch
                                checked={prefs.showMascot}
                                onCheckedChange={preferencesStore.setShowMascot}
                            />
                        </FieldRow>

                        <FieldRow
                            label="窗口不透明度"
                            description="80–100，仅作用于主背景；真窗口透明需 Tauri 配置（待）"
                        >
                            <input
                                type="range"
                                min={80}
                                max={100}
                                step={1}
                                value={prefs.windowOpacity}
                                onChange={(e) =>
                                    preferencesStore.setWindowOpacity(Number(e.target.value))
                                }
                                className="h-1 w-32 cursor-pointer accent-brand"
                            />
                            <span className="w-9 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                                {prefs.windowOpacity}%
                            </span>
                        </FieldRow>

                        <FieldRow
                            label="点击关闭按钮"
                            description="tray 模式需要 Tauri 系统托盘配套，当前选 tray 暂同 close"
                            isLast
                        >
                            <Select
                                value={prefs.closeAction}
                                onValueChange={(v) =>
                                    preferencesStore.setCloseAction(v as 'close' | 'tray')
                                }
                                items={[
                                    { value: 'close', label: '关闭程序' },
                                    { value: 'tray', label: '最小化到托盘' },
                                ]}
                            />
                        </FieldRow>
                    </TabsContent>

                    <TabsContent value="data" className="space-y-6">
                        <FieldRow label="数据根目录" description={dataRoot}>
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={handleOpen}
                                disabled={isOpeningDir}
                            >
                                打开
                            </Button>
                        </FieldRow>

                        <FieldRow
                            label="待接入功能"
                            description="GitHub PAT、离线通知（邮件 / Webhook）、配置导入导出、SnowLuma 全局密码 override 依赖后端 IPC，下个迭代落地"
                            isLast
                        >
                            <span className="text-[12px] text-text-tertiary">下次迭代</span>
                        </FieldRow>
                    </TabsContent>

                    <TabsContent value="about" className="space-y-6">
                        <FieldRow label="NapCatQQ Desktop" description="桌面端版本">
                            <span className="font-mono text-[12.5px] text-text-secondary">
                                {String(version)}
                            </span>
                        </FieldRow>

                        <FieldRow
                            label="许可"
                            description="本项目以 GPL-3.0 协议开源，详见仓库 LICENSE"
                            isLast
                        >
                            <span className="font-mono text-[12px] text-text-tertiary">GPL-3.0</span>
                        </FieldRow>
                    </TabsContent>
                </Tabs>
            </div>
        </div>
    );
}

export default SettingsPageNext;

// ============================================================================
// 子组件
// ============================================================================

/// shadcn Settings recipe 的标准行：左 label/description 堆叠 + 右控件，
/// space-between 自动右对齐。行间用底部 border + padding 切。
function FieldRow({
    label,
    description,
    isLast,
    children,
}: {
    label: string;
    description?: string;
    isLast?: boolean;
    children?: React.ReactNode;
}) {
    return (
        <div
            className={
                'flex items-center justify-between gap-6 ' +
                (isLast ? '' : 'border-b border-border-subtle pb-6')
            }
        >
            <div className="min-w-0 flex-1 space-y-1">
                <label className="block text-[13.5px] font-medium leading-none text-text">
                    {label}
                </label>
                {description && (
                    <p className="text-[12px] leading-relaxed text-text-tertiary">
                        {description}
                    </p>
                )}
            </div>
            {children && (
                <div className="flex shrink-0 items-center gap-2">{children}</div>
            )}
        </div>
    );
}

function ThemeSegment({
    value,
    onChange,
}: {
    value: ThemeMode;
    onChange: (next: ThemeMode) => void;
}) {
    const items: ReadonlyArray<{
        value: ThemeMode;
        label: string;
        icon: React.ReactNode;
    }> = [
            { value: 'auto', label: '系统', icon: <MonitorCog size={13} /> },
            { value: 'light', label: '浅色', icon: <Sun size={13} /> },
            { value: 'dark', label: '暗色', icon: <Moon size={13} /> },
        ];
    return (
        <div className="flex h-7 items-center rounded-md bg-inset p-0.5">
            {items.map((it) => (
                <button
                    key={it.value}
                    type="button"
                    onClick={() => onChange(it.value)}
                    className={
                        'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-colors ' +
                        (value === it.value
                            ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                            : 'text-text-tertiary hover:text-text')
                    }
                >
                    {it.icon}
                    <span>{it.label}</span>
                </button>
            ))}
        </div>
    );
}

// 客户端偏好 + 系统环境页。
//
// 视觉决策（对齐 Components 页节奏）：
//   - 紧凑 PageHeader 一行高度，不用大字标题 + 双副标题三段叠
//   - 单一外层 Card 包整页设置，分组内用细分割线 border-t 划分，不再卡中卡
//   - 主题用 segmented 按钮组（三选一），不用 RadioGroup（它把 description
//     和 label 挤在两行，单行高度 60+px，跟其它行不齐）
//   - Setting row 紧凑 h-10，行内右侧对齐控件，视觉跟 Components 行一样
//
// 数据来源都是纯前端 / 现有 IPC：
//   - usePreferences / preferencesStore  纯前端 localStorage
//   - useBootstrap.openDataDir + bootstrap.snapshot.data_root

import {
    Sun,
    Moon,
    MonitorCog,
    PanelsTopLeft,
    SlidersHorizontal,
    PowerOff,
    FolderOpen,
    Info,
    Beaker,
} from 'lucide-react';
import { Button, Card, InfoBar, Select, Switch } from '../../shared/ui';
import {
    preferencesStore,
    usePreferences,
    type ThemeMode,
} from '../../hooks/preferences/preferencesStore';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';

export function SettingsPageNext() {
    const prefs = usePreferences();
    const { bootstrap, openDataDir, isOpeningDir } = useBootstrap();

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
        <div className="flex h-full min-h-0 flex-col gap-3">
            <PageHeader />
            <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                <Card padding="none" className="overflow-hidden">
                    <Group title="外观">
                        <ThemeRow value={prefs.theme} onChange={preferencesStore.setTheme} />
                        <SwitchRow
                            icon={<PanelsTopLeft size={15} />}
                            label="主页吉祥物"
                            hint="关闭后概览页右上角不再显示猫娘"
                            value={prefs.showMascot}
                            onChange={preferencesStore.setShowMascot}
                        />
                        <SliderRow
                            icon={<SlidersHorizontal size={15} />}
                            label="窗口不透明度"
                            hint="80–100，仅影响主背景；真窗口透明需 Tauri 配置（待）"
                            value={prefs.windowOpacity}
                            onChange={preferencesStore.setWindowOpacity}
                        />
                    </Group>

                    <Group title="行为">
                        <SelectRow
                            icon={<PowerOff size={15} />}
                            label="点击关闭按钮时"
                            hint="tray 模式需要 Tauri 系统托盘配套，当前选 tray 暂同 close"
                            value={prefs.closeAction}
                            onChange={preferencesStore.setCloseAction}
                            items={[
                                { value: 'close', label: '关闭程序' },
                                { value: 'tray', label: '最小化到托盘' },
                            ]}
                        />
                    </Group>

                    <Group title="数据与版本">
                        <ActionRow
                            icon={<FolderOpen size={15} />}
                            label="数据根目录"
                            hint={dataRoot}
                        >
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={handleOpen}
                                disabled={isOpeningDir}
                            >
                                打开目录
                            </Button>
                        </ActionRow>
                        <ActionRow
                            icon={<Info size={15} />}
                            label="NapCatQQ Desktop"
                            hint="当前版本号"
                        >
                            <span className="font-mono text-[12px] text-text-tertiary">
                                {String(version)}
                            </span>
                        </ActionRow>
                    </Group>

                    <Group title="敬请期待">
                        <div className="px-4 py-3">
                            <InfoBar
                                tone="info"
                                title="待接入"
                                content="GitHub PAT、邮件 / Webhook 离线通知、配置导入导出、SnowLuma 全局密码 override 这些设置依赖后端 IPC，下个迭代落地"
                                closable={false}
                            />
                        </div>
                    </Group>
                </Card>
            </div>
        </div>
    );
}

export default SettingsPageNext;


// ============================================================================
// 子组件
// ============================================================================

function PageHeader() {
    return (
        <div className="flex items-baseline gap-3">
            <h1 className="text-[18px] font-semibold leading-none text-text">
                客户端偏好
            </h1>
            <span className="text-[12px] text-text-tertiary">
                外观主题 · 行为开关 · 数据目录与版本
            </span>
        </div>
    );
}

/// 一组设置项。第一组无顶 border，其它组用 border-t 切。
function Group({ title, children }: { title: string; children: React.ReactNode }) {
    return (
        <section className="border-t border-border-subtle first:border-t-0">
            <header className="flex items-center justify-between px-4 pt-3 pb-1.5">
                <h3 className="text-[11.5px] font-semibold uppercase tracking-[0.16em] text-text-tertiary">
                    {title}
                </h3>
            </header>
            <div className="flex flex-col">{children}</div>
        </section>
    );
}

/// 行框架。左侧 icon + label + hint，右侧自由插槽。
/// hint 长字符串走 truncate；默认 h-12（图标 + 两行）/ 紧凑场景外部传 h-10。
function SettingRow({
    icon,
    label,
    hint,
    children,
    align = 'center',
}: {
    icon?: React.ReactNode;
    label: string;
    hint?: string;
    children?: React.ReactNode;
    align?: 'center' | 'top';
}) {
    return (
        <div
            className={
                'flex items-' +
                (align === 'top' ? 'start' : 'center') +
                ' gap-3 px-4 py-2.5 transition-colors hover:bg-inset/50'
            }
        >
            {icon && (
                <span className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-sm bg-inset text-text-tertiary">
                    {icon}
                </span>
            )}
            <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium text-text">{label}</div>
                {hint && (
                    <div className="truncate text-[11.5px] text-text-tertiary" title={hint}>
                        {hint}
                    </div>
                )}
            </div>
            {children && (
                <div className="flex shrink-0 items-center gap-2">{children}</div>
            )}
        </div>
    );
}

function SwitchRow(props: {
    icon?: React.ReactNode;
    label: string;
    hint?: string;
    value: boolean;
    onChange: (next: boolean) => void;
}) {
    return (
        <SettingRow icon={props.icon} label={props.label} hint={props.hint}>
            <Switch checked={props.value} onCheckedChange={props.onChange} />
        </SettingRow>
    );
}

function SliderRow(props: {
    icon?: React.ReactNode;
    label: string;
    hint?: string;
    value: number;
    onChange: (next: number) => void;
}) {
    return (
        <SettingRow icon={props.icon} label={props.label} hint={props.hint}>
            <input
                type="range"
                min={80}
                max={100}
                step={1}
                value={props.value}
                onChange={(e) => props.onChange(Number(e.target.value))}
                className="h-1 w-32 cursor-pointer accent-brand"
            />
            <span className="w-10 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                {props.value}%
            </span>
        </SettingRow>
    );
}

function SelectRow<V extends string>(props: {
    icon?: React.ReactNode;
    label: string;
    hint?: string;
    value: V;
    onChange: (next: V) => void;
    items: ReadonlyArray<{ value: V; label: React.ReactNode }>;
}) {
    return (
        <SettingRow icon={props.icon} label={props.label} hint={props.hint}>
            <Select<V>
                value={props.value}
                onValueChange={props.onChange}
                items={props.items}
            />
        </SettingRow>
    );
}

function ActionRow(props: {
    icon?: React.ReactNode;
    label: string;
    hint?: string;
    children?: React.ReactNode;
}) {
    return (
        <SettingRow icon={props.icon} label={props.label} hint={props.hint}>
            {props.children}
        </SettingRow>
    );
}

/// 主题选择：segmented 三选一，跟 BotConfig 的 segment chip 风格一致。
function ThemeRow({
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
            { value: 'auto', label: '跟随系统', icon: <MonitorCog size={13} /> },
            { value: 'light', label: '浅色', icon: <Sun size={13} /> },
            { value: 'dark', label: '暗色', icon: <Moon size={13} /> },
        ];
    return (
        <SettingRow
            icon={<Beaker size={15} />}
            label="主题"
            hint="切换后立即生效，不需要重启"
        >
            <div className="flex h-7 items-center gap-0.5 rounded-md bg-inset p-0.5">
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
        </SettingRow>
    );
}

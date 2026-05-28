// 客户端偏好 + 系统环境页。
//
// IPC 不需要扩，能落地的都是纯前端偏好（落 localStorage）+ 已经暴露的
// useBootstrap 数据（数据目录 / 版本）。需要后端服务支撑的功能（邮件 /
// Webhook 通知、配置导入导出、SnowLuma 全局密码）暂留"敬请期待"占位。
//
// 视觉：跟 BotConfigPage.next 同套——卡片化分组（Card variant default）+
// FormSection 处理标题/描述。每段独立卡片，给信息层级低成本加深度。

import {
    Sun,
    Moon,
    MonitorCog,
    PanelsTopLeft,
    FolderOpen,
    Info,
} from 'lucide-react';
import {
    Button,
    Card,
    FormSection,
    InfoBar,
    RadioGroup,
    Select,
    Switch,
} from '../../shared/ui';
import {
    preferencesStore,
    usePreferences,
    type ThemeMode,
    type CloseAction,
} from '../../hooks/preferences/preferencesStore';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';

export function SettingsPageNext() {
    const prefs = usePreferences();
    const { bootstrap, openDataDir, isOpeningDir } = useBootstrap();

    return (
        <div className="flex h-full min-h-0 flex-col gap-3">
            <Header />
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
                <Card padding="md">
                    <AppearanceSection prefs={prefs} />
                </Card>
                <Card padding="md">
                    <BehaviorSection prefs={prefs} />
                </Card>
                <Card padding="md">
                    <DataAndAboutSection
                        bootstrap={bootstrap}
                        openDataDir={openDataDir}
                        isOpeningDir={isOpeningDir}
                    />
                </Card>
                <Card padding="md">
                    <PendingSection />
                </Card>
            </div>
        </div>
    );
}

export default SettingsPageNext;

// ─── 子组件 ─────────────────────────────────────────────────────────────────

function Header() {
    return (
        <div className="flex flex-col gap-1">
            <p className="text-[10px] font-semibold uppercase tracking-[0.2em] text-text-tertiary">
                Settings
            </p>
            <h1 className="text-[22px] font-semibold leading-tight text-text">
                客户端偏好
            </h1>
            <p className="text-[12.5px] text-text-tertiary">
                外观主题 / 行为开关 / 数据目录与版本信息
            </p>
        </div>
    );
}

function AppearanceSection({
    prefs,
}: {
    prefs: ReturnType<typeof usePreferences>;
}) {
    return (
        <FormSection
            title="外观"
            description="主题切换会立即生效，不需要重启"
        >
            <ThemeRow value={prefs.theme} onChange={preferencesStore.setTheme} />
            <MascotRow
                value={prefs.showMascot}
                onChange={preferencesStore.setShowMascot}
            />
            <OpacityRow
                value={prefs.windowOpacity}
                onChange={preferencesStore.setWindowOpacity}
            />
        </FormSection>
    );
}

function BehaviorSection({
    prefs,
}: {
    prefs: ReturnType<typeof usePreferences>;
}) {
    return (
        <FormSection
            title="行为"
            description="窗口与桌面集成相关"
        >
            <CloseActionRow
                value={prefs.closeAction}
                onChange={preferencesStore.setCloseAction}
            />
        </FormSection>
    );
}

function DataAndAboutSection({
    bootstrap,
    openDataDir,
    isOpeningDir,
}: {
    bootstrap: ReturnType<typeof useBootstrap>['bootstrap'];
    openDataDir: () => Promise<string>;
    isOpeningDir: boolean;
}) {
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
        <FormSection title="数据与版本">
            <Row
                icon={<FolderOpen size={16} className="text-text-tertiary" />}
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
            </Row>
            <Row
                icon={<Info size={16} className="text-text-tertiary" />}
                label="NapCatQQ Desktop 版本"
            >
                <span className="font-mono text-[11.5px] text-text-tertiary">
                    {String(version)}
                </span>
            </Row>
        </FormSection>
    );
}

function PendingSection() {
    return (
        <FormSection
            title="敬请期待"
            description="这些功能依赖后端 IPC，等下个迭代落地"
        >
            <InfoBar
                tone="info"
                title="待接入"
                content="GitHub PAT、邮件 / Webhook 离线通知、配置导入导出、SnowLuma 全局密码 override —— 这些设置需要后端服务支撑，当前先在前端留位"
                closable={false}
            />
        </FormSection>
    );
}

// ─── 行级控件 ────────────────────────────────────────────────────────────────

function Row({
    icon,
    label,
    hint,
    children,
}: {
    icon?: React.ReactNode;
    label: string;
    hint?: string;
    children?: React.ReactNode;
}) {
    return (
        <div className="flex items-center gap-3 py-1">
            {icon && <span className="shrink-0">{icon}</span>}
            <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium text-text">{label}</div>
                {hint && (
                    <div className="truncate text-[11.5px] text-text-tertiary" title={hint}>
                        {hint}
                    </div>
                )}
            </div>
            {children && <div className="flex shrink-0 items-center gap-2">{children}</div>}
        </div>
    );
}

function ThemeRow({
    value,
    onChange,
}: {
    value: ThemeMode;
    onChange: (next: ThemeMode) => void;
}) {
    const items: ReadonlyArray<{
        value: ThemeMode;
        label: React.ReactNode;
        description: string;
    }> = [
            {
                value: 'auto',
                label: (
                    <span className="inline-flex items-center gap-1.5">
                        <MonitorCog size={14} /> 跟随系统
                    </span>
                ),
                description: '按操作系统当前主题切换',
            },
            {
                value: 'light',
                label: (
                    <span className="inline-flex items-center gap-1.5">
                        <Sun size={14} /> 暖粉浅色
                    </span>
                ),
                description: '默认浅色主题',
            },
            {
                value: 'dark',
                label: (
                    <span className="inline-flex items-center gap-1.5">
                        <Moon size={14} /> 暖夜暗色
                    </span>
                ),
                description: '深色主题',
            },
        ];
    return (
        <div className="flex flex-col gap-1.5 py-1">
            <div className="text-[13px] font-medium text-text">主题</div>
            <RadioGroup<ThemeMode>
                value={value}
                onValueChange={onChange}
                items={items}
                orientation="horizontal"
            />
        </div>
    );
}

function MascotRow({
    value,
    onChange,
}: {
    value: boolean;
    onChange: (next: boolean) => void;
}) {
    return (
        <Row
            icon={<PanelsTopLeft size={16} className="text-text-tertiary" />}
            label="主页吉祥物"
            hint="关闭后概览页右上角不再显示猫娘"
        >
            <Switch checked={value} onCheckedChange={onChange} />
        </Row>
    );
}

function OpacityRow({
    value,
    onChange,
}: {
    value: number;
    onChange: (next: number) => void;
}) {
    return (
        <Row
            label="窗口不透明度"
            hint="80-100。仅影响主背景色，进一步真窗口透明需 Tauri 配置（待）"
        >
            <input
                type="range"
                min={80}
                max={100}
                step={1}
                value={value}
                onChange={(e) => onChange(Number(e.target.value))}
                className="h-1 w-32 cursor-pointer accent-brand"
            />
            <span className="w-10 text-right font-mono text-[11.5px] text-text-tertiary tabular-nums">
                {value}%
            </span>
        </Row>
    );
}

function CloseActionRow({
    value,
    onChange,
}: {
    value: CloseAction;
    onChange: (next: CloseAction) => void;
}) {
    return (
        <Row
            label="点击关闭按钮时"
            hint="tray 模式需要 Tauri 系统托盘配套，当前选 tray 暂同 close"
        >
            <Select<CloseAction>
                value={value}
                onValueChange={onChange}
                items={[
                    { value: 'close', label: '关闭程序' },
                    { value: 'tray', label: '最小化到托盘' },
                ]}
            />
        </Row>
    );
}

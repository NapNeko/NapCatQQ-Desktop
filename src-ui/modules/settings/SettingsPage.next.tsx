// 客户端偏好页。
//
// 视觉方向（参考 macOS Settings / Notion settings）：
//   - 页头跟 Components 页同节奏：components 小标 + 大字标题 + 副描述
//   - 整页一张大白卡 padding-lg，所有设置在卡内分段
//   - 段落标题 14px font-semibold，行间 border-b 极淡灰
//   - 行高 h-12，左 label/hint，右控件，无 hover bg、无 icon tile
//   - Card padding-lg 提供整体内边距，行不需要再 px-x

import { Sun, Moon, MonitorCog } from 'lucide-react';
import { Button, Card, Select, Switch } from '../../shared/ui';
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
        <div className="flex min-h-0 flex-1 flex-col">
            <header className="shrink-0 pb-4 pt-2">
                <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                    settings
                </p>
                <h1 className="font-display text-xl font-semibold text-text">
                    客户端偏好
                </h1>
                <p className="mt-1 text-sm text-text-secondary">
                    外观主题、行为开关、数据目录与版本信息。需要后端支撑的设置已收在底部「待接入」。
                </p>
            </header>

            <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                <Card padding="lg" className="mb-4 flex flex-col gap-7">
                    <Section title="外观">
                        <Row label="主题" hint="切换后立即生效，无需重启">
                            <ThemeSegment
                                value={prefs.theme}
                                onChange={preferencesStore.setTheme}
                            />
                        </Row>
                        <Row label="主页吉祥物" hint="概览页右上角的猫娘">
                            <Switch
                                checked={prefs.showMascot}
                                onCheckedChange={preferencesStore.setShowMascot}
                            />
                        </Row>
                        <Row
                            label="窗口不透明度"
                            hint="80–100，仅作用于主背景；真窗口透明需 Tauri 配置（待）"
                            last
                        >
                            <OpacitySlider
                                value={prefs.windowOpacity}
                                onChange={preferencesStore.setWindowOpacity}
                            />
                        </Row>
                    </Section>

                    <Section title="行为">
                        <Row
                            label="点击关闭按钮"
                            hint="tray 模式需要 Tauri 系统托盘配套，当前选 tray 暂同 close"
                            last
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
                        </Row>
                    </Section>

                    <Section title="数据与版本">
                        <Row label="数据根目录" hint={dataRoot}>
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={handleOpen}
                                disabled={isOpeningDir}
                            >
                                打开
                            </Button>
                        </Row>
                        <Row label="NapCatQQ Desktop 版本" last>
                            <span className="font-mono text-[12px] text-text-tertiary">
                                {String(version)}
                            </span>
                        </Row>
                    </Section>

                    <Section title="待接入">
                        <p className="text-[12.5px] leading-relaxed text-text-tertiary">
                            GitHub PAT、离线通知（邮件 / Webhook）、配置导入导出、
                            SnowLuma 全局密码 override 等设置依赖后端 IPC，下个迭代落地。
                        </p>
                    </Section>
                </Card>
            </div>
        </div>
    );
}

export default SettingsPageNext;


// ============================================================================
// 子组件
// ============================================================================

function Section({ title, children }: { title: string; children: React.ReactNode }) {
    return (
        <section className="flex flex-col">
            <h3 className="mb-1 text-[14px] font-semibold leading-none text-text">
                {title}
            </h3>
            <div className="flex flex-col">{children}</div>
        </section>
    );
}

function Row({
    label,
    hint,
    last,
    children,
}: {
    label: string;
    hint?: string;
    last?: boolean;
    children?: React.ReactNode;
}) {
    return (
        <div
            className={
                'flex min-h-[48px] items-center gap-4 py-2.5 ' +
                (last ? '' : 'border-b border-border-subtle')
            }
        >
            <div className="min-w-0 flex-1">
                <div className="text-[13.5px] text-text">{label}</div>
                {hint && (
                    <div className="mt-0.5 truncate text-[12px] text-text-tertiary" title={hint}>
                        {hint}
                    </div>
                )}
            </div>
            {children && <div className="flex shrink-0 items-center gap-2">{children}</div>}
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

function OpacitySlider({
    value,
    onChange,
}: {
    value: number;
    onChange: (next: number) => void;
}) {
    return (
        <>
            <input
                type="range"
                min={80}
                max={100}
                step={1}
                value={value}
                onChange={(e) => onChange(Number(e.target.value))}
                className="h-1 w-32 cursor-pointer accent-brand"
            />
            <span className="w-9 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                {value}%
            </span>
        </>
    );
}

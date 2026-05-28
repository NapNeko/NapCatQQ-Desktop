// 客户端偏好页。
//
// 视觉哲学：长文档读法，不画框。
// 整页只有间距 + 字号 + 一根行间细线，不画卡片、不画组容器。眼睛靠空白
// 自然区分段落，跟 BotConfigPage.next 的 FormSection 同套思路。
//
// 数据：纯前端 preferences + useBootstrap.openDataDir / data_root。
// 后端 IPC 待接入的功能用一段灰色注释样的"待接入"列表交代，不画 InfoBar。

import { Sun, Moon, MonitorCog } from 'lucide-react';
import { Button, Select, Switch } from '../../shared/ui';
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
        <div className="flex h-full min-h-0 flex-col gap-1">
            <Header />
            <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                <div className="mx-auto flex max-w-[640px] flex-col gap-10 py-6">
                    <Section title="外观">
                        <Row label="主题">
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
                        <Row label="窗口不透明度" hint="80–100">
                            <OpacitySlider
                                value={prefs.windowOpacity}
                                onChange={preferencesStore.setWindowOpacity}
                            />
                        </Row>
                    </Section>

                    <Section title="行为">
                        <Row label="关闭按钮">
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

                    <Section title="数据">
                        <Row label="数据目录" hint={dataRoot}>
                            <Button
                                variant="ghost"
                                size="sm"
                                onClick={handleOpen}
                                disabled={isOpeningDir}
                            >
                                打开
                            </Button>
                        </Row>
                    </Section>

                    <Section title="关于">
                        <Row label="NapCatQQ Desktop">
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
                </div>
            </div>
        </div>
    );
}

export default SettingsPageNext;


// ============================================================================
// 子组件
// ============================================================================

function Header() {
    return (
        <div className="flex items-baseline gap-3 pb-2">
            <h1 className="text-[16px] font-semibold leading-none text-text">
                偏好
            </h1>
            <span className="text-[12px] text-text-tertiary">客户端</span>
        </div>
    );
}

/// 段落：标题（半粗深灰）+ 内容垂直堆叠。无卡片、无外框、无背景色。
/// 段落之间靠父级 gap-10 区分；标题靠字重 + 字号区分。
function Section({ title, children }: { title: string; children: React.ReactNode }) {
    return (
        <section className="flex flex-col gap-3">
            <h3 className="text-[13px] font-semibold leading-none text-text-secondary">
                {title}
            </h3>
            <div className="flex flex-col">{children}</div>
        </section>
    );
}

/// 单行：左 label（13px 主色）+ 可选 hint（12px 灰）/ 右控件。
/// 行间用 border-b 极淡灰线分隔，最后一行不画线。无 hover 态。
function Row({
    label,
    hint,
    children,
}: {
    label: string;
    hint?: string;
    children?: React.ReactNode;
}) {
    return (
        <div className="flex min-h-[36px] items-center gap-3 border-b border-border-subtle py-2 last:border-b-0">
            <div className="min-w-0 flex-1">
                <div className="text-[13px] text-text">{label}</div>
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

/// 主题 segmented：3 选 1，最低视觉重量。
/// 不用 inset 底色（那是"控件容器"语义），用纯 ghost 按钮 + 选中态 brand。
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
        <div className="flex h-7 items-center gap-1">
            {items.map((it) => (
                <button
                    key={it.value}
                    type="button"
                    onClick={() => onChange(it.value)}
                    className={
                        'flex h-7 items-center gap-1 rounded-sm px-2 text-[12px] transition-colors ' +
                        (value === it.value
                            ? 'text-brand'
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

// 终端 Tab：字号、光标、配色、右键、粘贴、高亮。改了立刻生效，不走右上角的「保存设置」。

import { Button, Select, Switch } from '../../../shared/ui';
import {
    DEFAULT_TERMINAL_PREFS,
    terminalPrefs,
    useTerminalPrefs,
    type TerminalColorScheme,
    type TerminalCursorStyle,
    type TerminalRightClick,
} from '../../../hooks/terminal/terminalPrefs';
import { useTerminalLaunchOptions } from '../../../hooks/terminal/useTerminalLaunchOptions';
import type { LocalShellKind } from '../../../core/ipc/generated/domain/LocalShellKind';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

const FONT_SIZES = [10, 11, 12, 13, 14, 15, 16, 18, 20];
const LINE_HEIGHTS = [1, 1.1, 1.15, 1.25, 1.4];
const SCROLLBACKS = [1000, 5000, 10000, 50000];

const SHORTCUTS: [string, string][] = [
    ['Ctrl+`', '打开 / 收起终端面板'],
    ['Ctrl+Shift+`', '新开一个本机终端'],
    ['Ctrl+Shift+F', '在输出里搜索'],
    ['Ctrl+↑ / Ctrl+↓', '跳到上一条 / 下一条命令'],
    ['Ctrl+C', '有选中时复制，没有时中断程序'],
    ['Ctrl+V / Shift+Insert', '粘贴'],
    ['Ctrl+滚轮 / Ctrl+= / Ctrl+-', '放大 / 缩小，Ctrl+0 还原'],
    ['Ctrl+点击链接', '用浏览器打开'],
    ['中键点标签', '关掉这个标签'],
];

export function TerminalTab() {
    const prefs = useTerminalPrefs();
    const { shells } = useTerminalLaunchOptions(true);
    const patch = terminalPrefs.patch;

    return (
        <SettingsTabSections>
            <SettingsSection title="外观">
                <FieldRow label="字号">
                    <Select
                        value={String(prefs.fontSize)}
                        onValueChange={(v) => patch({ fontSize: Number(v) })}
                        items={FONT_SIZES.map((n) => ({ value: String(n), label: `${n}` }))}
                    />
                </FieldRow>
                <FieldRow label="行高">
                    <Select
                        value={String(prefs.lineHeight)}
                        onValueChange={(v) => patch({ lineHeight: Number(v) })}
                        items={LINE_HEIGHTS.map((n) => ({
                            value: String(n),
                            label: n.toFixed(2).replace(/0$/, ''),
                        }))}
                    />
                </FieldRow>
                <FieldRow label="配色">
                    <Select
                        value={prefs.colorScheme}
                        onValueChange={(v) => patch({ colorScheme: v as TerminalColorScheme })}
                        items={[
                            { value: 'auto', label: '跟着界面主题' },
                            { value: 'dark', label: '始终深色' },
                        ]}
                    />
                </FieldRow>
                <FieldRow label="光标">
                    <Select
                        value={prefs.cursorStyle}
                        onValueChange={(v) => patch({ cursorStyle: v as TerminalCursorStyle })}
                        items={[
                            { value: 'bar', label: '竖线' },
                            { value: 'block', label: '方块' },
                            { value: 'underline', label: '下划线' },
                        ]}
                    />
                </FieldRow>
                <FieldRow label="光标闪烁">
                    <Switch
                        checked={prefs.cursorBlink}
                        onCheckedChange={(v) => patch({ cursorBlink: v })}
                    />
                </FieldRow>
                <FieldRow
                    label="关键字高亮"
                    description="错误、警告、成功字样和 IP、链接自动上色；程序自己上了色的不动"
                >
                    <Switch
                        checked={prefs.highlight}
                        onCheckedChange={(v) => patch({ highlight: v })}
                    />
                </FieldRow>
                <FieldRow
                    label="显卡加速"
                    description="字多、刷屏快时更顺；显示不正常时关掉"
                    isLast
                >
                    <Switch checked={prefs.gpu} onCheckedChange={(v) => patch({ gpu: v })} />
                </FieldRow>
            </SettingsSection>

            <SettingsSection title="操作">
                <FieldRow label="本机默认 shell">
                    <Select
                        value={prefs.defaultShell ?? 'auto'}
                        onValueChange={(v) =>
                            patch({ defaultShell: v === 'auto' ? null : (v as LocalShellKind) })
                        }
                        items={[
                            { value: 'auto', label: '自动（PowerShell 7 优先）' },
                            ...shells.map((s) => ({ value: s.kind, label: s.label })),
                        ]}
                    />
                </FieldRow>
                <FieldRow label="右键">
                    <Select
                        value={prefs.rightClick}
                        onValueChange={(v) => patch({ rightClick: v as TerminalRightClick })}
                        items={[
                            { value: 'menu', label: '弹出菜单' },
                            { value: 'paste', label: '有选中就复制，没有就粘贴' },
                        ]}
                    />
                </FieldRow>
                <FieldRow label="选中即复制">
                    <Switch
                        checked={prefs.copyOnSelect}
                        onCheckedChange={(v) => patch({ copyOnSelect: v })}
                    />
                </FieldRow>
                <FieldRow label="粘贴多行前先确认" description="防止一粘贴就连着执行好几条命令">
                    <Switch
                        checked={prefs.confirmMultilinePaste}
                        onCheckedChange={(v) => patch({ confirmMultilinePaste: v })}
                    />
                </FieldRow>
                <FieldRow label="回滚行数" isLast>
                    <Select
                        value={String(prefs.scrollback)}
                        onValueChange={(v) => patch({ scrollback: Number(v) })}
                        items={SCROLLBACKS.map((n) => ({ value: String(n), label: `${n} 行` }))}
                    />
                </FieldRow>
            </SettingsSection>

            <SettingsSection title="快捷键">
                <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 py-1 text-[13px]">
                    {SHORTCUTS.map(([keys, action]) => (
                        <div key={keys} className="contents">
                            <dt>
                                <kbd className="rounded-xs bg-inset px-1.5 py-0.5 text-[12px] text-text">
                                    {keys}
                                </kbd>
                            </dt>
                            <dd className="text-text-secondary">{action}</dd>
                        </div>
                    ))}
                </dl>
            </SettingsSection>

            <div>
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                        terminalPrefs.patch({ ...DEFAULT_TERMINAL_PREFS, snippets: prefs.snippets })
                    }
                >
                    恢复默认（保留我的命令）
                </Button>
            </div>
        </SettingsTabSections>
    );
}

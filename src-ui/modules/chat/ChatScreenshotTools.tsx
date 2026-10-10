// 截图与设置共用一个紧凑入口，组合键录制期间暂停截图快捷键。
import { useId, useState } from 'react';
import { ChevronDown, RotateCcw, Scissors } from 'lucide-react';
import {
    DEFAULT_SCREENSHOT_PREFERENCES,
    screenshotShortcutFromEvent,
} from '../../core/domain/chat/chatScreenshotPreferences';
import type { useChatScreenshot } from '../../hooks/chat/useChatScreenshot';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { Button } from '../../shared/ui/Button';
import { Popover, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import { Switch } from '../../shared/ui/Switch';
import './chat-screenshot.css';

export function ChatScreenshotTools({
    screenshot,
    collapsed,
}: {
    screenshot: ReturnType<typeof useChatScreenshot>;
    collapsed: boolean;
}) {
    const [open, setOpen] = useState(false);
    const [recording, setRecording] = useState(false);
    const hideId = useId();
    const globalId = useId();
    const shortcutId = useId();
    const addToChatId = useId();
    const stopRecording = () => {
        setRecording(false);
        screenshot.suspendShortcut(false);
    };
    return (
        <div className="native-chat-screenshot-tools">
            <Button
                variant="ghost"
                size="icon"
                className="native-chat-icon"
                aria-label="截图"
                title={`截图 · ${screenshot.preferences.shortcut}`}
                aria-busy={screenshot.capturing || undefined}
                disabled={screenshot.capturing || collapsed}
                onClick={() => {
                    setOpen(false);
                    void screenshot.start();
                }}
            >
                <Scissors size={18} />
            </Button>
            <Popover
                open={open && !collapsed}
                onOpenChange={(value) => {
                    setOpen(value);
                    if (!value) stopRecording();
                }}
            >
                <PopoverTrigger asChild>
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon native-chat-screenshot-options"
                        aria-label="截图设置"
                        title="截图设置"
                        disabled={screenshot.capturing || collapsed}
                    >
                        <ChevronDown size={11} />
                    </Button>
                </PopoverTrigger>
                <PopoverContent
                    side="top"
                    align="start"
                    className="native-chat-popover native-chat-screenshot-settings"
                    aria-label="截图设置"
                    onCloseAutoFocus={stopRecording}
                >
                    <div className="native-chat-screenshot-heading">
                        <Scissors size={14} />
                        <span>截图</span>
                        <Button
                            variant="ghost"
                            size="icon"
                            className="native-chat-screenshot-reset"
                            aria-label="恢复默认"
                            title="恢复默认"
                            onClick={() => {
                                screenshot.updatePreferences(DEFAULT_SCREENSHOT_PREFERENCES);
                                stopRecording();
                            }}
                        >
                            <RotateCcw size={14} />
                        </Button>
                    </div>
                    <div className="native-chat-screenshot-row">
                        <label htmlFor={addToChatId}>截图后添加到聊天框</label>
                        <Switch
                            id={addToChatId}
                            checked={screenshot.preferences.addToChat}
                            onCheckedChange={(addToChat) =>
                                screenshot.updatePreferences({ addToChat })
                            }
                        />
                    </div>
                    <div className="native-chat-screenshot-row">
                        <label htmlFor={hideId}>截图时隐藏聊天窗口</label>
                        <Switch
                            id={hideId}
                            checked={screenshot.preferences.hideWindow}
                            onCheckedChange={(hideWindow) =>
                                screenshot.updatePreferences({ hideWindow })
                            }
                        />
                    </div>
                    <div className="native-chat-screenshot-row">
                        <label htmlFor={globalId}>全局快捷键</label>
                        <Switch
                            id={globalId}
                            checked={screenshot.preferences.globalShortcut}
                            onCheckedChange={(globalShortcut) =>
                                screenshot.updatePreferences({ globalShortcut })
                            }
                        />
                    </div>
                    <div className="native-chat-screenshot-shortcut-row">
                        <div>
                            <label htmlFor={shortcutId}>快捷键</label>
                            <span>
                                {screenshot.preferences.globalShortcut
                                    ? '全局响应'
                                    : '程序有焦点时响应'}
                            </span>
                        </div>
                        <button
                            id={shortcutId}
                            type="button"
                            className="native-chat-screenshot-shortcut"
                            data-chat-screenshot-shortcut
                            data-recording={recording}
                            aria-label="修改截图快捷键"
                            aria-pressed={recording}
                            onClick={() => {
                                setRecording(true);
                                screenshot.suspendShortcut(true);
                            }}
                            onBlur={stopRecording}
                            onKeyDown={(event) => {
                                if (!recording || event.nativeEvent.isComposing) return;
                                event.preventDefault();
                                event.stopPropagation();
                                if (event.key === 'Escape') {
                                    stopRecording();
                                    return;
                                }
                                if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
                                const shortcut = screenshotShortcutFromEvent(event);
                                if (!shortcut) {
                                    pushInfoBar({
                                        key: 'chat:screenshot:shortcut-input',
                                        tone: 'warning',
                                        title: '请选择组合键',
                                        content: '使用 Ctrl 或 Alt 加字母、数字或 F1–F12',
                                    });
                                    return;
                                }
                                screenshot.updatePreferences({ shortcut });
                                stopRecording();
                            }}
                        >
                            {recording ? (
                                <span role="status">按下组合键</span>
                            ) : (
                                screenshot.preferences.shortcut
                                    .split('+')
                                    .map((key) => <kbd key={key}>{key}</kbd>)
                            )}
                        </button>
                    </div>
                </PopoverContent>
            </Popover>
        </div>
    );
}

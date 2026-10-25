// 登录只在用户点击后写入可见终端；终端关闭时提供复制命令。

import { useRef, useState } from 'react';
import type { AssistantLoginPlan } from '../../core/domain/assistant/types';
import { errorText } from '../../core/domain/errors';
import { assistantService } from '../../core/services/assistant.service';
import { clipboardService } from '../../core/services/clipboard.service';
import { terminalService } from '../../core/services/terminal.service';
import { featureTogglesStore, useFeatureEnabled } from '../preferences/featureTogglesStore';
import { terminalStore } from '../terminal/terminalStore';

const shellLabels: Record<AssistantLoginPlan['shell'], string> = {
    pwsh: 'PowerShell 7',
    windows_powershell: 'Windows PowerShell',
    cmd: '命令提示符',
    git_bash: 'Git Bash',
    wsl: 'WSL',
};

export function useAssistantLogin(sessionId: string) {
    const terminalEnabled = useFeatureEnabled('terminal');
    const [busy, setBusy] = useState(false);
    const [plan, setPlan] = useState<AssistantLoginPlan | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [feedback, setFeedback] = useState<string | null>(null);
    const pending = useRef(false);
    const start = async () => {
        if (pending.current) return;
        setError(null);
        setFeedback(null);
        if (assistantService.preview) {
            setFeedback('浏览器预览不执行登录，请在桌面端操作。');
            return;
        }
        pending.current = true;
        setBusy(true);
        try {
            const next = await assistantService.loginPlan(sessionId);
            setPlan(next);
            const copy = async () => {
                await clipboardService.writeText(next.command);
                setFeedback(`登录命令已复制，请在 ${shellLabels[next.shell]} 中运行。`);
            };
            if (!featureTogglesStore.getSnapshot().terminal) {
                await copy();
                return;
            }
            const terminalId = await terminalStore.open(
                { kind: 'local' },
                { shell: next.shell, forceNew: true },
            );
            if (!terminalId) throw new Error('本机终端未能打开，请重试或手动复制下方命令。');
            // 等待终端创建时，用户可能刚好关闭了该功能；此时不提交认证命令。
            if (!featureTogglesStore.getSnapshot().terminal) {
                await copy();
                return;
            }
            await terminalService.write(terminalId, `${next.command}\r`);
            setFeedback('登录命令已提交到终端。完成登录后，回到此会话发送新消息。');
        } catch (cause) {
            setError(errorText(cause));
        } finally {
            pending.current = false;
            setBusy(false);
        }
    };
    return {
        start,
        busy,
        error,
        feedback,
        terminalEnabled,
        preview: assistantService.preview,
        command: plan?.command ?? null,
        shellLabel: plan ? shellLabels[plan.shell] : null,
    };
}

// 会话 id → xterm 运行时。跟着会话表增删：新会话建实例并接上输出，关掉的会话销毁实例。
// 主题、偏好改了一起推给所有实例；Windows 版本号探到后补给本机的实例（xterm 按它决定能不能重排折行）。

import type { ITheme } from '@xterm/xterm';
import { terminalPrefs } from '../../hooks/terminal/terminalPrefs';
import { terminalStore } from '../../hooks/terminal/terminalStore';
import { TerminalRuntime } from './runtime';

const runtimes = new Map<string, TerminalRuntime>();
let theme: ITheme = { background: '#1d1916', foreground: '#e9e2d8' };
// ConPTY 从 21376 起才支持折行重排；探不到就按 Windows 10 算（关掉重排，不丢行）
let windowsBuild = 19045;
let started = 0;

export function getRuntime(id: string): TerminalRuntime | undefined {
    return runtimes.get(id);
}

export function setTerminalTheme(next: ITheme) {
    theme = next;
    for (const runtime of runtimes.values()) runtime.applyTheme(next);
}

function sync() {
    const sessions = terminalStore.getSnapshot().sessions;
    for (const [id, runtime] of runtimes) {
        if (!sessions[id]) {
            runtime.dispose();
            runtimes.delete(id);
        }
    }
    for (const [id, view] of Object.entries(sessions)) {
        if (runtimes.has(id)) continue;
        const runtime = new TerminalRuntime(view.info, theme, windowsBuild);
        runtimes.set(id, runtime);
        void runtime.attach();
    }
}

async function detectWindowsBuild() {
    const uad = (
        navigator as Navigator & {
            userAgentData?: {
                getHighEntropyValues?: (
                    hints: string[],
                ) => Promise<{ platform?: string; platformVersion?: string }>;
            };
        }
    ).userAgentData;
    if (!uad?.getHighEntropyValues) return;
    try {
        const values = await uad.getHighEntropyValues(['platform', 'platformVersion']);
        const major = Number.parseInt(values.platformVersion?.split('.')[0] ?? '', 10);
        // Chromium 在 Windows 11 上报 13 及以上
        if (values.platform === 'Windows' && major >= 13) {
            windowsBuild = 22000;
            for (const runtime of runtimes.values()) runtime.setWindowsBuild(windowsBuild);
        }
    } catch {
        // 保持按 Windows 10 算
    }
}

/** 应用根上调一次：接上已有会话，之后跟着会话表走 */
export function startTerminalRuntimes(): () => void {
    started++;
    if (started > 1)
        return () => {
            started--;
        };
    void detectWindowsBuild();
    const unsubscribeStore = terminalStore.subscribe(sync);
    let lastPrefs = terminalPrefs.get();
    const unsubscribePrefs = terminalPrefs.subscribe(() => {
        const prefs = terminalPrefs.get();
        if (prefs === lastPrefs) return;
        lastPrefs = prefs;
        for (const runtime of runtimes.values()) runtime.applyPrefs(prefs);
    });
    sync();
    void terminalStore.bootstrap();
    return () => {
        started--;
        if (started > 0) return;
        unsubscribeStore();
        unsubscribePrefs();
    };
}

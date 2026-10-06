// 一个终端会话对应的 xterm 实例。
//
// 活在模块里而不是 React 组件里：切标签、收起面板、换页面都不毁，只把 DOM 挪进挪出。
// 还没显示过的会话也先接上输出（只进缓冲区，不渲染），后台标签才亮得起活动点，sudo 提示也能及时认出。
// WebGL 渲染只给挂在界面上的那几个（浏览器能开的 WebGL 上下文有限），摘下来就退回 DOM。

import { Terminal, type IDisposable, type IMarker, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import { WebglAddon } from '@xterm/addon-webgl';
import { ClipboardAddon, type IClipboardProvider } from '@xterm/addon-clipboard';
import { createKeywordHighlighter } from '../../core/domain/terminal/highlight';
import {
    parseOsc133,
    parseOsc633,
    parseOsc7,
    parseOsc9,
    type ShellMark,
} from '../../core/domain/terminal/osc';
import { appendTail, isSudoPrompt, lastLine } from '../../core/domain/terminal/sudoPrompt';
import { targetKey } from '../../core/domain/terminal/commands';
import { isDarkColor } from '../../core/domain/terminal/palette';
import { terminalIo } from '../../hooks/terminal/terminalIo';
import { terminalPrefs, type TerminalPrefs } from '../../hooks/terminal/terminalPrefs';
import { terminalRecents } from '../../hooks/terminal/terminalRecents';
import { isLive, terminalStore } from '../../hooks/terminal/terminalStore';
import type { TerminalEventEnvelope } from '../../core/ipc/generated/domain/TerminalEventEnvelope';
import type { TerminalSessionInfo } from '../../core/ipc/generated/domain/TerminalSessionInfo';

export const TERMINAL_FONT =
    '"JetBrains Mono Variable", "Cascadia Mono", Consolas, "Microsoft YaHei Mono", "Microsoft YaHei UI", "PingFang SC", monospace';

const FAIL_COLOR = '#e85b57';
const ACK_BATCH = 128 * 1024;

export interface TerminalViewHooks {
    /** 多行粘贴前问一下；返回 false 就不贴 */
    confirmPaste(text: string): Promise<boolean>;
    /** Ctrl+Shift+F */
    openSearch(): void;
}

interface CommandMark {
    marker: IMarker;
    code: number | null;
    command: string;
}

let fontsPromise: Promise<void> | null = null;
function fontsReady(): Promise<void> {
    if (!fontsPromise) {
        const load = document.fonts?.load?.('400 13px "JetBrains Mono Variable"');
        fontsPromise = (load ?? Promise.resolve()).then(
            () => undefined,
            () => undefined,
        );
    }
    return fontsPromise;
}

export class TerminalRuntime {
    readonly id: string;
    readonly term: Terminal;
    private info: TerminalSessionInfo;
    private readonly host: HTMLDivElement;
    private readonly fit = new FitAddon();
    private readonly search = new SearchAddon();
    private webgl: WebglAddon | null = null;
    private opened = false;
    private mounted = false;
    private disposed = false;
    private readonly decoder = new TextDecoder('utf-8');
    private readonly highlighter = createKeywordHighlighter();
    private tail = '';
    private promptMarker: IMarker | null = null;
    private pendingCommand: string | null = null;
    private runningCommand: string | null = null;
    private marks: CommandMark[] = [];
    private unacked = 0;
    private ackTimer: number | null = null;
    private resizeTimer: number | null = null;
    private readonly disposables: IDisposable[] = [];
    private darkTheme = true;
    view: TerminalViewHooks | null = null;

    constructor(info: TerminalSessionInfo, theme: ITheme, windowsBuild: number) {
        this.id = info.id as string;
        this.info = info;
        this.host = document.createElement('div');
        this.host.className = 'ncd-term-host';
        const prefs = terminalPrefs.get();
        this.darkTheme = isDarkColor(theme.background ?? '#000000');
        this.term = new Terminal({
            allowProposedApi: true,
            cols: 120,
            rows: 30,
            fontFamily: TERMINAL_FONT,
            fontSize: prefs.fontSize,
            lineHeight: prefs.lineHeight,
            cursorStyle: prefs.cursorStyle,
            cursorBlink: prefs.cursorBlink,
            cursorInactiveStyle: 'outline',
            scrollback: prefs.scrollback,
            theme,
            minimumContrastRatio: 4.5,
            overviewRuler: { width: 8 },
            rightClickSelectsWord: false,
            drawBoldTextInBrightColors: false,
            smoothScrollDuration: 0,
            fastScrollSensitivity: 5,
            scrollOnUserInput: true,
            windowsPty:
                info.host_os === 'windows'
                    ? { backend: 'conpty', buildNumber: windowsBuild }
                    : undefined,
        });
        this.term.loadAddon(this.fit);
        this.term.loadAddon(this.search);
        this.term.loadAddon(new Unicode11Addon());
        this.term.unicode.activeVersion = '11';
        this.term.loadAddon(
            new WebLinksAddon((event, uri) => {
                if (event.ctrlKey || event.metaKey) terminalIo.openLink(uri);
            }),
        );
        // OSC 52：远端 vim / tmux 复制的东西写进本机剪贴板；不让远端程序读本机剪贴板
        const clipboard: IClipboardProvider = {
            readText: () => '',
            writeText: (_selection, text) =>
                navigator.clipboard.writeText(text).catch(() => undefined),
        };
        this.term.loadAddon(new ClipboardAddon(undefined, clipboard));

        const parser = this.term.parser;
        this.disposables.push(
            parser.registerOscHandler(633, (data) => this.mark(parseOsc633(data))),
            parser.registerOscHandler(133, (data) => this.mark(parseOsc133(data))),
            parser.registerOscHandler(7, (data) => this.mark(parseOsc7(data))),
            parser.registerOscHandler(9, (data) => this.mark(parseOsc9(data))),
            this.term.onData((data) => this.input(data)),
            this.term.onBinary((data) => this.input(data)),
            this.term.onSelectionChange(() => {
                if (terminalPrefs.get().copyOnSelect && this.term.hasSelection())
                    this.copySelection(false);
            }),
        );
        this.term.attachCustomKeyEventHandler((ev) => this.key(ev));
        this.host.addEventListener('paste', this.onPaste, true);
        this.host.addEventListener('contextmenu', this.onContextMenu);
        // 捕获阶段：Ctrl+滚轮要在 xterm 滚动之前截下来改字号
        this.host.addEventListener('wheel', this.onWheel, { passive: false, capture: true });
    }

    /** 接上后端：先收回放，再收实时输出 */
    async attach(): Promise<void> {
        try {
            const info = await terminalIo.attach(this.id, {
                output: (bytes) => this.output(bytes),
                event: (envelope) => this.event(envelope),
            });
            this.info = info;
            terminalStore.updateInfo(this.id, info);
        } catch {
            terminalStore.setStatus(this.id, { kind: 'disconnected', reason: '接不上这个终端' });
        }
    }

    mount(container: HTMLElement) {
        if (this.disposed) return;
        if (this.host.parentElement !== container) container.appendChild(this.host);
        this.mounted = true;
        void fontsReady().then(() => {
            if (!this.mounted || this.disposed || !this.host.isConnected) return;
            if (!this.opened) {
                this.term.open(this.host);
                this.opened = true;
            }
            this.enableGpu();
            this.fitNow();
            this.term.refresh(0, this.term.rows - 1);
        });
    }

    unmount() {
        this.mounted = false;
        this.disableGpu();
        this.host.remove();
    }

    focus() {
        if (this.opened) this.term.focus();
    }

    fitNow() {
        if (!this.opened || !this.host.isConnected || this.host.clientWidth === 0) return;
        const dims = this.fit.proposeDimensions();
        if (!dims || !Number.isFinite(dims.cols) || !Number.isFinite(dims.rows)) return;
        const cols = Math.max(2, dims.cols);
        const rows = Math.max(1, dims.rows);
        if (cols === this.term.cols && rows === this.term.rows) return;
        this.term.resize(cols, rows);
        if (this.resizeTimer !== null) window.clearTimeout(this.resizeTimer);
        this.resizeTimer = window.setTimeout(() => {
            this.resizeTimer = null;
            terminalIo.resize(this.id, this.term.cols, this.term.rows);
        }, 60);
    }

    applyPrefs(prefs: TerminalPrefs) {
        const o = this.term.options;
        o.fontSize = prefs.fontSize;
        o.lineHeight = prefs.lineHeight;
        o.cursorStyle = prefs.cursorStyle;
        o.cursorBlink = prefs.cursorBlink;
        o.scrollback = prefs.scrollback;
        if (!prefs.gpu) this.disableGpu();
        else if (this.mounted) this.enableGpu();
        this.fitNow();
    }

    applyTheme(theme: ITheme) {
        this.darkTheme = isDarkColor(theme.background ?? '#000000');
        this.term.options.theme = theme;
    }

    setWindowsBuild(build: number) {
        if (this.info.host_os === 'windows')
            this.term.options.windowsPty = { backend: 'conpty', buildNumber: build };
    }

    /** 往输入行里填（常用命令、最近命令、拖进来的路径），不带回车 */
    fillInput(text: string) {
        if (!isLive(this.info.status)) return;
        this.term.paste(text);
        this.focus();
    }

    async paste(text: string) {
        if (!text) return;
        const multiline = /[\r\n]/.test(text.replace(/[\r\n]+$/, ''));
        if (multiline && terminalPrefs.get().confirmMultilinePaste && this.view) {
            const ok = await this.view.confirmPaste(text);
            if (!ok) return;
        }
        this.term.paste(text);
        this.focus();
    }

    async pasteFromClipboard() {
        await this.paste(await terminalIo.readClipboard());
    }

    copySelection(clear = true) {
        const text = this.term.getSelection();
        if (text) void navigator.clipboard.writeText(text).catch(() => undefined);
        if (clear) this.term.clearSelection();
    }

    selectAll() {
        this.term.selectAll();
    }

    /** 清屏，连后端的回放一起清，重新接上时不会再冒出来 */
    clear() {
        this.term.clear();
        this.marks = this.marks.filter((m) => !m.marker.isDisposed);
        terminalIo.clearHistory(this.id);
    }

    find(
        query: string,
        direction: 'next' | 'prev',
        options: { caseSensitive: boolean; regex: boolean; wholeWord: boolean },
    ): boolean {
        if (!query) {
            this.search.clearDecorations();
            return false;
        }
        const decorations = this.darkTheme
            ? {
                  matchBackground: '#5a3446',
                  activeMatchBackground: '#9c4f6e',
                  matchOverviewRuler: '#c76a8e',
                  activeMatchColorOverviewRuler: '#f58fb6',
              }
            : {
                  matchBackground: '#fde2ec',
                  activeMatchBackground: '#f9a3c5',
                  matchOverviewRuler: '#e070a0',
                  activeMatchColorOverviewRuler: '#c76a8e',
              };
        const opts = { ...options, decorations, incremental: direction === 'next' };
        return direction === 'next'
            ? this.search.findNext(query, opts)
            : this.search.findPrevious(query, opts);
    }

    clearSearch() {
        this.search.clearDecorations();
    }

    onSearchResults(
        listener: (result: { resultIndex: number; resultCount: number }) => void,
    ): IDisposable {
        return this.search.onDidChangeResults(listener);
    }

    /** 跳到上一条 / 下一条命令的提示符 */
    jumpToCommand(direction: -1 | 1) {
        const top = this.term.buffer.active.viewportY;
        const lines = this.marks
            .filter((m) => !m.marker.isDisposed)
            .map((m) => m.marker.line)
            .concat(
                this.promptMarker && !this.promptMarker.isDisposed ? [this.promptMarker.line] : [],
            );
        const target =
            direction < 0
                ? Math.max(...lines.filter((l) => l < top), -1)
                : Math.min(...lines.filter((l) => l > top), Number.POSITIVE_INFINITY);
        if (target >= 0 && Number.isFinite(target)) this.term.scrollToLine(target);
        else if (direction > 0) this.term.scrollToBottom();
    }

    /** 整个缓冲区的纯文本（导出用），折行接回原来的一行 */
    bufferText(): string {
        const buffer = this.term.buffer.active;
        const lines: string[] = [];
        for (let i = 0; i < buffer.length; i++) {
            const line = buffer.getLine(i);
            if (!line) continue;
            const text = line.translateToString(true);
            if (line.isWrapped && lines.length > 0) lines[lines.length - 1] += text;
            else lines.push(text);
        }
        return `${lines.join('\n').replace(/\s+$/, '')}\n`;
    }

    dispose() {
        if (this.disposed) return;
        this.disposed = true;
        if (this.ackTimer !== null) window.clearTimeout(this.ackTimer);
        if (this.resizeTimer !== null) window.clearTimeout(this.resizeTimer);
        this.flushAck();
        this.host.removeEventListener('paste', this.onPaste, true);
        this.host.removeEventListener('contextmenu', this.onContextMenu);
        this.host.removeEventListener('wheel', this.onWheel, true);
        this.disposables.forEach((d) => d.dispose());
        this.disableGpu();
        this.term.dispose();
        this.host.remove();
    }

    // ---- 内部 ----

    private output(bytes: Uint8Array) {
        if (this.disposed) return;
        const text = this.decoder.decode(bytes, { stream: true });
        const shown = terminalPrefs.get().highlight ? this.highlighter.process(text) : text;
        this.term.write(shown, () => this.ack(bytes.length));
        this.tail = appendTail(this.tail, text);
        if (this.info.features.sudo_fill) {
            terminalStore.setSudoPrompt(
                this.id,
                isSudoPrompt(lastLine(this.tail), this.runningCommand),
            );
        }
        if (!terminalStore.isVisible(this.id)) terminalStore.setActivity(this.id, 'output');
    }

    private event(envelope: TerminalEventEnvelope) {
        if (envelope.kind === 'status') {
            this.info = { ...this.info, status: envelope.status };
            terminalStore.setStatus(this.id, envelope.status);
            if (!isLive(envelope.status)) this.resetShellState();
        } else if (envelope.kind === 'info') {
            this.info = envelope.info;
            terminalStore.updateInfo(this.id, envelope.info);
            this.resetShellState();
        }
    }

    private resetShellState() {
        this.highlighter.reset();
        this.promptMarker = null;
        this.pendingCommand = null;
        this.runningCommand = null;
        this.tail = '';
    }

    private ack(bytes: number) {
        this.unacked += bytes;
        if (this.unacked >= ACK_BATCH) {
            this.flushAck();
        } else if (this.ackTimer === null) {
            this.ackTimer = window.setTimeout(() => this.flushAck(), 16);
        }
    }

    private flushAck() {
        if (this.ackTimer !== null) {
            window.clearTimeout(this.ackTimer);
            this.ackTimer = null;
        }
        if (this.unacked > 0) {
            terminalIo.ack(this.id, this.unacked);
            this.unacked = 0;
        }
    }

    private input(data: string) {
        if (!isLive(this.info.status)) {
            // 已经退出 / 断线：按回车重开
            if (data.includes('\r')) void terminalStore.restart(this.id);
            return;
        }
        terminalIo.write(this.id, data);
    }

    private mark(mark: ShellMark | null): boolean {
        if (!mark) return false;
        switch (mark.kind) {
            case 'prompt_start':
                this.promptMarker = this.term.registerMarker(0);
                break;
            case 'command_line':
                this.pendingCommand = mark.command;
                break;
            case 'command_start':
                this.runningCommand = this.pendingCommand;
                break;
            case 'command_end':
                this.finishCommand(mark.exitCode);
                break;
            case 'cwd':
                terminalStore.setCwd(this.id, mark.path);
                break;
            case 'progress':
                terminalStore.setProgress(
                    this.id,
                    mark.state === 0 ? null : { state: mark.state, value: mark.value },
                );
                break;
            case 'prompt_end':
                break;
        }
        return true;
    }

    private finishCommand(code: number | null) {
        const marker = this.promptMarker;
        const command = this.pendingCommand ?? '';
        this.pendingCommand = null;
        this.runningCommand = null;
        if (marker && !marker.isDisposed && code !== null) {
            const fail = code !== 0;
            const decoration = this.term.registerDecoration({
                marker,
                width: 1,
                overviewRulerOptions: fail ? { color: FAIL_COLOR, position: 'full' } : undefined,
            });
            decoration?.onRender((el) => {
                el.classList.add('ncd-term-mark');
                el.classList.toggle('is-fail', fail);
                el.title = `${fail ? `退出码 ${code}` : '成功'}${command ? ` · ${command}` : ''}`;
            });
            this.marks.push({ marker, code, command });
        }
        if (command) terminalRecents.remember(targetKey(this.info.target), command);
        if (!terminalStore.isVisible(this.id)) {
            terminalStore.setActivity(this.id, code === null || code === 0 ? 'ok' : 'fail');
        }
        terminalStore.setSudoPrompt(this.id, false);
    }

    private key(ev: KeyboardEvent): boolean {
        if (ev.type !== 'keydown') return true;
        const ctrl = ev.ctrlKey || ev.metaKey;
        const key = ev.key.toLowerCase();
        if (ctrl && !ev.altKey) {
            if (key === 'c' && this.term.hasSelection()) {
                ev.preventDefault();
                this.copySelection();
                return false;
            }
            if (key === 'c' && ev.shiftKey) return false;
            // 交给浏览器粘贴，再由 onPaste 统一处理（多行确认、括号粘贴）
            if (key === 'v') return false;
            if (ev.shiftKey && key === 'f') {
                ev.preventDefault();
                this.view?.openSearch();
                return false;
            }
            if (ev.code === 'Backquote') {
                ev.preventDefault();
                if (ev.shiftKey) void terminalStore.open({ kind: 'local' }, { forceNew: true });
                else terminalStore.toggle();
                return false;
            }
            if (key === '=' || key === '+' || key === '-' || key === '0') {
                ev.preventDefault();
                terminalPrefs.zoom(key === '0' ? 0 : key === '-' ? -1 : 1);
                return false;
            }
            if ((ev.key === 'ArrowUp' || ev.key === 'ArrowDown') && !ev.shiftKey) {
                ev.preventDefault();
                this.jumpToCommand(ev.key === 'ArrowUp' ? -1 : 1);
                return false;
            }
        }
        if (ev.shiftKey && ev.key === 'Insert') return false;
        return true;
    }

    private readonly onPaste = (ev: ClipboardEvent) => {
        const text = ev.clipboardData?.getData('text/plain') ?? '';
        ev.preventDefault();
        ev.stopImmediatePropagation();
        void this.paste(text);
    };

    /// 右键设成「复制 / 粘贴」时在这里就处理掉；设成菜单时不碰事件，由外面的 ContextMenu 打开
    /// （Radix 看到 defaultPrevented 就不开菜单了）
    private readonly onContextMenu = (ev: MouseEvent) => {
        if (terminalPrefs.get().rightClick !== 'paste') return;
        ev.preventDefault();
        ev.stopPropagation();
        if (this.term.hasSelection()) this.copySelection();
        else void this.pasteFromClipboard();
    };

    hasSelection(): boolean {
        return this.term.hasSelection();
    }

    private readonly onWheel = (ev: WheelEvent) => {
        if (!(ev.ctrlKey || ev.metaKey)) return;
        ev.preventDefault();
        ev.stopPropagation();
        terminalPrefs.zoom(ev.deltaY < 0 ? 1 : -1);
    };

    private enableGpu() {
        if (!terminalPrefs.get().gpu || this.webgl || !this.opened) return;
        try {
            const addon = new WebglAddon();
            addon.onContextLoss(() => {
                addon.dispose();
                if (this.webgl === addon) this.webgl = null;
            });
            this.term.loadAddon(addon);
            this.webgl = addon;
        } catch {
            this.webgl = null;
        }
    }

    private disableGpu() {
        const addon = this.webgl;
        this.webgl = null;
        addon?.dispose();
    }
}

// 内嵌终端 IPC。对接 src-tauri/src/commands/terminal.rs；浏览器预览走 mock 里的假 shell。
//
// 输出走 Channel 原始字节（小块 eval 成 ArrayBuffer，大块走 fetch），状态事件走另一条 JSON 通道。
// 前端每写完一批回 ack，后端据此流控。

import { Channel, invoke, isTauri, pickAnyFiles, saveFileAs } from '../ipc/transport';
import { terminalMock } from '../ipc/mock/terminal.mock';
import type { LocalShellOption } from '../ipc/generated/domain/LocalShellOption';
import type { ServerStats } from '../ipc/generated/domain/ServerStats';
import type { TerminalDirListing } from '../ipc/generated/domain/TerminalDirListing';
import type { TerminalEventEnvelope } from '../ipc/generated/domain/TerminalEventEnvelope';
import type { TerminalOpenRequest } from '../ipc/generated/domain/TerminalOpenRequest';
import type { TerminalSessionInfo } from '../ipc/generated/domain/TerminalSessionInfo';
import type { TerminalTextFile } from '../ipc/generated/domain/TerminalTextFile';

export interface TerminalAttachHandlers {
    output(bytes: Uint8Array): void;
    event(envelope: TerminalEventEnvelope): void;
}

function toBytes(message: unknown): Uint8Array {
    if (message instanceof ArrayBuffer) return new Uint8Array(message);
    if (ArrayBuffer.isView(message)) {
        return new Uint8Array(message.buffer, message.byteOffset, message.byteLength);
    }
    if (Array.isArray(message)) return Uint8Array.from(message as number[]);
    return new Uint8Array();
}

export const terminalService = {
    localShells: (): Promise<LocalShellOption[]> =>
        isTauri ? invoke('terminal_local_shells') : terminalMock.localShells(),

    list: (): Promise<TerminalSessionInfo[]> =>
        isTauri ? invoke('terminal_list') : terminalMock.list(),

    open: (request: TerminalOpenRequest): Promise<TerminalSessionInfo> =>
        isTauri ? invoke('terminal_open', { request }) : terminalMock.open(request),

    /// 接上一个会话：先收一遍回放，之后是实时输出。再接一次会顶掉上一个接收方
    attach: (id: string, handlers: TerminalAttachHandlers): Promise<TerminalSessionInfo> => {
        if (!isTauri) return terminalMock.attach(id, handlers);
        const output = new Channel<unknown>();
        output.onmessage = (message) => handlers.output(toBytes(message));
        const events = new Channel<TerminalEventEnvelope>();
        events.onmessage = (envelope) => handlers.event(envelope);
        return invoke('terminal_attach', { id, output, events });
    },

    write: (id: string, data: string): Promise<void> =>
        isTauri ? invoke('terminal_write', { id, data }) : terminalMock.write(id, data),

    resize: (id: string, cols: number, rows: number): Promise<void> =>
        isTauri
            ? invoke('terminal_resize', { id, cols, rows })
            : terminalMock.resize(id, cols, rows),

    ack: (id: string, bytes: number): Promise<void> =>
        isTauri ? invoke('terminal_ack', { id, bytes }) : Promise.resolve(),

    restart: (id: string): Promise<TerminalSessionInfo> =>
        isTauri ? invoke('terminal_restart', { id }) : terminalMock.restart(id),

    close: (id: string): Promise<void> =>
        isTauri ? invoke('terminal_close', { id }) : terminalMock.close(id),

    clearHistory: (id: string): Promise<void> =>
        isTauri ? invoke('terminal_clear_history', { id }) : Promise.resolve(),

    fillSudo: (id: string): Promise<void> =>
        isTauri ? invoke('terminal_fill_sudo', { id }) : terminalMock.fillSudo(id),

    openExternal: (request: TerminalOpenRequest): Promise<void> =>
        isTauri ? invoke('terminal_open_external', { request }) : Promise.resolve(),

    stats: (id: string): Promise<ServerStats> =>
        isTauri ? invoke('terminal_stats', { id }) : terminalMock.stats(id),

    listDir: (id: string, path: string): Promise<TerminalDirListing> =>
        isTauri ? invoke('terminal_list_dir', { id, path }) : terminalMock.listDir(id, path),

    readText: (id: string, path: string): Promise<TerminalTextFile> =>
        isTauri ? invoke('terminal_read_text', { id, path }) : terminalMock.readText(id, path),

    writeText: (id: string, path: string, content: string, crlf: boolean): Promise<void> =>
        isTauri
            ? invoke('terminal_write_text', { id, path, content, crlf })
            : terminalMock.writeText(id, path, content),

    makeDir: (id: string, path: string): Promise<void> =>
        isTauri ? invoke('terminal_make_dir', { id, path }) : terminalMock.makeDir(id, path),

    rename: (id: string, from: string, to: string): Promise<void> =>
        isTauri ? invoke('terminal_rename', { id, from, to }) : terminalMock.rename(id, from, to),

    remove: (id: string, path: string, isDir: boolean): Promise<void> =>
        isTauri ? invoke('terminal_remove', { id, path, isDir }) : terminalMock.remove(id, path),

    upload: (id: string, localPaths: string[], destDir: string): Promise<number> =>
        isTauri
            ? invoke('terminal_upload', { id, localPaths, destDir })
            : Promise.resolve(localPaths.length),

    download: (id: string, path: string, localDest: string): Promise<void> =>
        isTauri ? invoke('terminal_download', { id, path, localDest }) : Promise.resolve(),

    exportText: (path: string, content: string): Promise<void> =>
        isTauri ? invoke('terminal_export_text', { path, content }) : Promise.resolve(),

    readClipboard: (): Promise<string> =>
        isTauri
            ? invoke('read_clipboard_text')
            : (navigator.clipboard?.readText?.().catch(() => '') ?? Promise.resolve('')),

    pickUploadFiles: (): Promise<string[]> =>
        isTauri ? pickAnyFiles('选择要上传的文件') : Promise.resolve([]),

    pickSaveTarget: (
        title: string,
        defaultName: string,
        filters: { name: string; extensions: string[] }[] = [],
    ): Promise<string | null> =>
        isTauri ? saveFileAs(title, defaultName, filters) : Promise.resolve(null),
};

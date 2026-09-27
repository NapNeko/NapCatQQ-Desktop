// 终端运行时（modules 里的 xterm 实例）要用的 IPC，从这里转一手：modules 层不直接碰 services。

import { terminalService, type TerminalAttachHandlers } from '../../core/services/terminal.service';
import { openExternalUrl } from '../../core/ipc/transport';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export type { TerminalAttachHandlers };

const quiet = (p: Promise<unknown>) => void p.catch(() => undefined);

export const terminalIo = {
    attach: terminalService.attach,
    write: (id: string, data: string) => quiet(terminalService.write(id, data)),
    resize: (id: string, cols: number, rows: number) => quiet(terminalService.resize(id, cols, rows)),
    ack: (id: string, bytes: number) => quiet(terminalService.ack(id, bytes)),
    clearHistory: (id: string) => quiet(terminalService.clearHistory(id)),
    readClipboard: () => terminalService.readClipboard().catch(() => ''),

    async fillSudo(id: string) {
        try {
            await terminalService.fillSudo(id);
        } catch (err) {
            pushErrorBar({ title: '没能填入密码', raw: errorText(err) });
        }
    },

    openLink(url: string) {
        openExternalUrl(url).catch((err) => pushErrorBar({ title: '无法打开链接', raw: errorText(err) }));
    },

    async exportText(defaultName: string, content: string) {
        try {
            const path = await terminalService.pickSaveTarget('导出终端输出', defaultName, [
                { name: '文本', extensions: ['txt', 'log'] },
            ]);
            if (!path) return;
            await terminalService.exportText(path, content);
            pushInfoBar({ tone: 'success', title: '已导出', content: path, autoDismissMs: 4000 });
        } catch (err) {
            pushErrorBar({ title: '导出失败', raw: errorText(err) });
        }
    },
};

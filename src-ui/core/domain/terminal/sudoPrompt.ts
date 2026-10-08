// 认出 sudo 在等密码：出一个「填入密码」的浮按钮，点了由后端把这台主机存的提权密码直接写进终端
// （密码不经过网页）。
//
// sudo 自己的提示（`[sudo] password for u:`，中文系统 `[sudo] u 的密码：`）一定认；
// 光秃秃的 `Password:` / `密码：` 只有正在跑的命令里带 sudo / su 时才认，
// 免得在 ssh 别的机器、mysql -p 这种要别的密码的地方误导人去填提权密码。

const ESCAPES =
    // eslint-disable-next-line no-control-regex -- ESC/BEL 字面量就是 ANSI 解析目标
    /\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[P^_X][^\x1b]*\x1b\\|[ -/]*[0-~])/g;

export function stripAnsi(text: string): string {
    return text.replace(ESCAPES, '');
}

const SUDO_OWN = [/\[sudo\] password for [^\s:]+:\s*$/i, /\[sudo\] \S+ 的密码[:：]\s*$/];
const GENERIC = [/(?:^|\s)[Pp]assword(?: for [^\s:]+)?:\s*$/, /密码[:：]\s*$/];
const ELEVATING = /(?:^|[\s;&|(])(?:sudo|su)(?:\s|$)/;

/** 最后一行（光标所在的那行）的纯文本，最多留 256 个字符 */
export function lastLine(tail: string): string {
    const plain = stripAnsi(tail);
    const cut = Math.max(plain.lastIndexOf('\n'), plain.lastIndexOf('\r'));
    const line = cut === -1 ? plain : plain.slice(cut + 1);
    return line.length > 256 ? line.slice(-256) : line;
}

export function isSudoPrompt(line: string, runningCommand: string | null): boolean {
    if (SUDO_OWN.some((re) => re.test(line))) return true;
    if (!runningCommand || !ELEVATING.test(runningCommand)) return false;
    return GENERIC.some((re) => re.test(line));
}

/** 输出流里攒一小段尾巴，够判断最后一行就行 */
export function appendTail(tail: string, chunk: string, max = 1024): string {
    const next = tail + chunk;
    return next.length > max ? next.slice(-max) : next;
}

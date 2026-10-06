import { describe, expect, it } from 'vitest';
import { appendTail, isSudoPrompt, lastLine, stripAnsi } from './sudoPrompt';
import { pushRecent, shouldRemember, targetKey } from './commands';
import {
    baseName,
    breadcrumbs,
    navDirection,
    cdCommand,
    editorSyntaxOf,
    invalidFileName,
    joinHostPath,
    looksLikeText,
    quotePath,
    shellSyntaxOf,
    toWslPath,
} from './paths';
import { formatBytes, formatModified, formatUptime, loadTone, percent } from './format';
import { buildPalette, isDarkColor, withAlpha } from './palette';

describe('sudo prompt', () => {
    it('recognises sudo prompts on the last line only', () => {
        expect(
            isSudoPrompt(
                lastLine('\x1b[1m$ sudo apt update\r\n[sudo] password for napcat: '),
                null,
            ),
        ).toBe(true);
        expect(isSudoPrompt(lastLine('[sudo] napcat 的密码： '), null)).toBe(true);
        expect(isSudoPrompt(lastLine('[sudo] password for u: \r\nok\r\n$ '), null)).toBe(false);
    });

    it('bare Password: only counts while an elevating command runs', () => {
        expect(isSudoPrompt('Password: ', 'sudo -k ls')).toBe(true);
        expect(isSudoPrompt('Password: ', 'su -')).toBe(true);
        expect(isSudoPrompt('Password: ', 'ssh root@other')).toBe(false);
        expect(isSudoPrompt('Enter password: ', 'mysql -u root -p')).toBe(false);
        expect(isSudoPrompt('Password: ', null)).toBe(false);
    });

    it('strips escape sequences and keeps a bounded tail', () => {
        expect(stripAnsi('\x1b[31mred\x1b[0m \x1b]0;title\x07x')).toBe('red x');
        expect(appendTail('a'.repeat(1020), 'bcdefg', 1024)).toHaveLength(1024);
    });
});

describe('recent commands', () => {
    it('skips secrets and space-prefixed lines', () => {
        expect(shouldRemember('uv pip list')).toBe(true);
        expect(shouldRemember(' rm -rf x')).toBe(false);
        expect(shouldRemember('export API_KEY=abc')).toBe(false);
        expect(shouldRemember('mysql -u root -pS3cret')).toBe(false);
        expect(shouldRemember('curl -H "Authorization: Bearer x" u')).toBe(false);
        expect(shouldRemember('')).toBe(false);
    });

    it('moves repeats to the front and caps the list', () => {
        expect(pushRecent(['a', 'b', 'c'], 'b')).toEqual(['b', 'a', 'c']);
        expect(pushRecent(['a', 'b'], 'c', 2)).toEqual(['c', 'a']);
    });

    it('keys targets', () => {
        expect(targetKey({ kind: 'local' })).toBe('local');
        expect(targetKey({ kind: 'bot', bot_id: '1', host_dir: true })).toBe('bot:1:host');
        expect(targetKey({ kind: 'app_instance', instance_id: 'm1' })).toBe('app:m1');
    });
});

describe('paths', () => {
    it('quotes per shell', () => {
        expect(quotePath('/srv/app', 'posix')).toBe('/srv/app');
        expect(quotePath("/srv/it's here", 'posix')).toBe(`'/srv/it'\\''s here'`);
        expect(quotePath('C:\\Program Files\\x', 'powershell')).toBe(`'C:\\Program Files\\x'`);
        expect(quotePath("C:\\a'b", 'powershell')).toBe(`'C:\\a''b'`);
        expect(quotePath('C:\\a b', 'cmd')).toBe('"C:\\a b"');
        expect(quotePath('C:\\Users\\x y', 'wsl')).toBe(`'/mnt/c/Users/x y'`);
        expect(toWslPath('D:\\data')).toBe('/mnt/d/data');
    });

    it('builds cd commands and picks the syntax', () => {
        expect(cdCommand('C:\\x y', 'powershell')).toBe(`Set-Location -LiteralPath 'C:\\x y'`);
        expect(cdCommand('C:\\x', 'cmd')).toBe('cd /d C:\\x');
        expect(cdCommand('/home/u', 'posix')).toBe('cd -- /home/u');
        expect(shellSyntaxOf('linux', undefined)).toBe('posix');
        expect(shellSyntaxOf('windows', 'git_bash')).toBe('posix');
        expect(shellSyntaxOf('windows', undefined)).toBe('powershell');
    });

    it('joins, names and classifies files', () => {
        expect(joinHostPath('linux', '/', 'etc')).toBe('/etc');
        expect(joinHostPath('windows', 'C:\\data', 'x')).toBe('C:\\data\\x');
        expect(baseName('/home/u/app/')).toBe('app');
        expect(baseName('C:\\x\\y.txt')).toBe('y.txt');
        expect(invalidFileName('a/b')).toBe(true);
        expect(invalidFileName('..')).toBe(true);
        expect(invalidFileName('ok.txt')).toBe(false);
        expect(looksLikeText('bot_config.toml')).toBe(true);
        expect(looksLikeText('.env.prod')).toBe(true);
        expect(looksLikeText('qq.png')).toBe(false);
        expect(editorSyntaxOf('a.json')).toBe('json');
        expect(editorSyntaxOf('.env')).toBe('dot_env');
    });

    it('splits paths into clickable crumbs', () => {
        expect(breadcrumbs('windows', 'D:\\apps\\karin')).toEqual([
            { label: '此电脑', path: '' },
            { label: 'D:', path: 'D:\\' },
            { label: 'apps', path: 'D:\\apps' },
            { label: 'karin', path: 'D:\\apps\\karin' },
        ]);
        expect(breadcrumbs('windows', '')).toEqual([{ label: '此电脑', path: '' }]);
        expect(breadcrumbs('windows', 'C:\\')).toEqual([
            { label: '此电脑', path: '' },
            { label: 'C:', path: 'C:\\' },
        ]);
        expect(breadcrumbs('linux', '/home/u')).toEqual([
            { label: '/', path: '/' },
            { label: 'home', path: '/home' },
            { label: 'u', path: '/home/u' },
        ]);
    });

    it('tells going into a folder from going back', () => {
        expect(navDirection('/home', '/home/u')).toBe('in');
        expect(navDirection('/', '/etc')).toBe('in');
        expect(navDirection('/home/u', '/home')).toBe('out');
        expect(navDirection('/home/u', '/home/uu')).toBe('out');
        expect(navDirection('', 'C:\\')).toBe('in');
        expect(navDirection('C:\\', '')).toBe('out');
        expect(navDirection('C:\\', 'C:\\Users')).toBe('in');
        expect(navDirection('C:\\Users', 'D:\\')).toBe('out');
    });
});

describe('format', () => {
    it('formats sizes, rates and uptime', () => {
        expect(formatBytes(512)).toBe('512 B');
        expect(formatBytes(1536)).toBe('1.5 KB');
        expect(formatBytes(3 * 1024 ** 3)).toBe('3.0 GB');
        expect(formatUptime(90061)).toBe('1 天 1 小时');
        expect(formatUptime(3900)).toBe('1 小时 5 分');
        expect(percent(1, 4)).toBe(25);
        expect(loadTone(95)).toBe('danger');
        expect(loadTone(80)).toBe('warn');
        expect(loadTone(10)).toBe('normal');
    });

    it('shortens modification times', () => {
        const now = new Date(2026, 8, 27, 18, 0);
        expect(formatModified(new Date(2026, 8, 27, 9, 5).getTime() / 1000, now)).toBe('09:05');
        expect(formatModified(new Date(2026, 2, 3).getTime() / 1000, now)).toBe('3-03');
        expect(formatModified(new Date(2024, 0, 2).getTime() / 1000, now)).toBe('2024-01-02');
    });
});

describe('palette', () => {
    it('follows the theme background and can be forced dark', () => {
        const light = buildPalette(
            { background: '#f4efe7', foreground: '#2c1f18', accent: '#f58fb6' },
            'auto',
        );
        expect(light.background).toBe('#f4efe7');
        expect(light.red).toBe('#c8373a');
        const forced = buildPalette(
            { background: '#f4efe7', foreground: '#2c1f18', accent: '#f58fb6' },
            'dark',
        );
        expect(isDarkColor(forced.background)).toBe(true);
        expect(forced.red).toBe('#f0716b');
        expect(withAlpha('#ff0000', 0.5)).toBe('#ff000080');
    });
});

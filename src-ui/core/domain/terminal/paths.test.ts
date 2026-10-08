import { describe, expect, it } from 'vitest';
import type { LocalShellKind } from '../../ipc/generated/domain/LocalShellKind';
import {
    DRIVES_PATH,
    baseName,
    breadcrumbs,
    cdCommand,
    editorSyntaxOf,
    invalidFileName,
    isDrivesView,
    joinHostPath,
    looksLikeText,
    navDirection,
    quotePath,
    shellSyntaxOf,
    toWslPath,
} from './paths';

describe('shellSyntaxOf', () => {
    it('Linux 主机一律 posix，不管选了什么 shell', () => {
        for (const shell of [undefined, 'pwsh', 'cmd', 'wsl'] as (LocalShellKind | undefined)[]) {
            expect(shellSyntaxOf('linux', shell)).toBe('posix');
        }
    });

    it('Windows 按 shell 分流，默认 powershell', () => {
        expect(shellSyntaxOf('windows', 'cmd')).toBe('cmd');
        expect(shellSyntaxOf('windows', 'git_bash')).toBe('posix');
        expect(shellSyntaxOf('windows', 'wsl')).toBe('wsl');
        expect(shellSyntaxOf('windows', 'pwsh')).toBe('powershell');
        expect(shellSyntaxOf('windows', 'windows_powershell')).toBe('powershell');
        expect(shellSyntaxOf('windows', undefined)).toBe('powershell');
    });
});

describe('toWslPath', () => {
    it('盘符路径挂到 /mnt/<盘小写>', () => {
        expect(toWslPath('C:\\Users\\napcat\\NapCat\\config')).toBe(
            '/mnt/c/Users/napcat/NapCat/config',
        );
        expect(toWslPath('D:/data/napcat')).toBe('/mnt/d/data/napcat');
        expect(toWslPath('C:\\')).toBe('/mnt/c/');
    });

    it('非盘符路径只换分隔符', () => {
        expect(toWslPath('\\\\server\\share\\app')).toBe('//server/share/app');
        expect(toWslPath('relative\\dir')).toBe('relative/dir');
        expect(toWslPath('/home/u')).toBe('/home/u');
    });
});

describe('quotePath', () => {
    it('安全字符直通，空格 / 引号才加壳', () => {
        expect(quotePath('/opt/napcat/config', 'posix')).toBe('/opt/napcat/config');
        expect(quotePath('C:\\ProgramData\\NapCatQQ Desktop', 'powershell')).toBe(
            `'C:\\ProgramData\\NapCatQQ Desktop'`,
        );
    });

    it('posix 单引号内再套单引号用 \'"\'" 转义', () => {
        expect(quotePath("/srv/it's app", 'posix')).toBe(`'/srv/it'\\''s app'`);
    });

    it('powershell 单引号翻倍，cmd 剥掉内嵌双引号', () => {
        expect(quotePath("C:\\a'b", 'powershell')).toBe(`'C:\\a''b'`);
        expect(quotePath('C:\\a"b', 'cmd')).toBe('"C:\\ab"');
        expect(quotePath('C:\\a b', 'cmd')).toBe('"C:\\a b"');
    });

    it('wsl 先转 /mnt 路径再按 posix 引号规则', () => {
        expect(quotePath('C:\\Users\\x y', 'wsl')).toBe(`'/mnt/c/Users/x y'`);
        expect(quotePath('C:\\a\\b', 'wsl')).toBe('/mnt/c/a/b');
    });
});

describe('cdCommand', () => {
    it('三种 shell 各自的 cd 写法', () => {
        expect(cdCommand('/home/napcat', 'posix')).toBe('cd -- /home/napcat');
        expect(cdCommand('C:\\x y', 'powershell')).toBe(`Set-Location -LiteralPath 'C:\\x y'`);
        expect(cdCommand('C:\\x', 'cmd')).toBe('cd /d C:\\x');
        expect(cdCommand('C:\\x y', 'wsl')).toBe(`cd -- '/mnt/c/x y'`);
    });
});

describe('joinHostPath 与 baseName', () => {
    it('按主机选分隔符，目录自带尾分隔符不重复', () => {
        expect(joinHostPath('linux', '/home/napcat', 'NapCat')).toBe('/home/napcat/NapCat');
        expect(joinHostPath('linux', '/', 'etc')).toBe('/etc');
        expect(joinHostPath('windows', 'C:\\NapCat', 'webapp')).toBe('C:\\NapCat\\webapp');
        expect(joinHostPath('windows', 'C:\\', 'NapCat')).toBe('C:\\NapCat');
    });

    it('取末段并容忍尾部分隔符', () => {
        expect(baseName('/home/napcat/NapCat/')).toBe('NapCat');
        expect(baseName('C:\\NapCat\\webapp\\package.json')).toBe('package.json');
        expect(baseName('napcat.json')).toBe('napcat.json');
        expect(baseName('/')).toBe('');
    });
});

describe('此电脑视图与面包屑', () => {
    it('空路径只在 Windows 表示「此电脑」', () => {
        expect(isDrivesView('windows', DRIVES_PATH)).toBe(true);
        expect(isDrivesView('windows', null)).toBe(false);
        expect(isDrivesView('windows', 'C:\\')).toBe(false);
        expect(isDrivesView('linux', DRIVES_PATH)).toBe(false);
    });

    it('Windows 每级路径都是可点的完整前缀', () => {
        expect(breadcrumbs('windows', 'C:\\Program Files\\NapCat')).toEqual([
            { label: '此电脑', path: '' },
            { label: 'C:', path: 'C:\\' },
            { label: 'Program Files', path: 'C:\\Program Files' },
            { label: 'NapCat', path: 'C:\\Program Files\\NapCat' },
        ]);
        expect(breadcrumbs('windows', 'D:\\apps\\')).toEqual([
            { label: '此电脑', path: '' },
            { label: 'D:', path: 'D:\\' },
            { label: 'apps', path: 'D:\\apps' },
        ]);
        expect(breadcrumbs('windows', '')).toEqual([{ label: '此电脑', path: '' }]);
    });

    it('Linux 从 / 起步逐级拼接', () => {
        expect(breadcrumbs('linux', '/opt/napcat/config')).toEqual([
            { label: '/', path: '/' },
            { label: 'opt', path: '/opt' },
            { label: 'napcat', path: '/opt/napcat' },
            { label: 'config', path: '/opt/napcat/config' },
        ]);
        expect(breadcrumbs('linux', '/')).toEqual([{ label: '/', path: '/' }]);
    });
});

describe('navDirection', () => {
    it('进出盘与子目录判 in，回退 / 平级跳转判 out', () => {
        expect(navDirection(DRIVES_PATH, 'C:\\')).toBe('in');
        expect(navDirection('C:\\', DRIVES_PATH)).toBe('out');
        expect(navDirection('C:\\Users', 'C:\\Users\\napcat')).toBe('in');
        expect(navDirection('C:\\Users\\napcat', 'C:\\Users')).toBe('out');
        expect(navDirection('C:\\Users', 'C:\\Windows')).toBe('out');
        expect(navDirection('/home/napcat', '/home/napcatter')).toBe('out');
        expect(navDirection('/home', '/home')).toBe('out');
    });
});

describe('文件名与类型判定', () => {
    it('拦 Windows 非法字符与控制字符，空名 / 点名也算非法', () => {
        for (const name of [
            '',
            '  ',
            '.',
            '  ..  ',
            'a/b',
            'a\\b',
            'a:b',
            'a*b',
            'a?b',
            'a"b',
            'a<b',
            'a>b',
            'a|b',
            'a\x01b',
        ]) {
            expect(invalidFileName(name)).toBe(true);
        }
        expect(invalidFileName('onebot11_10001.json')).toBe(false);
        expect(invalidFileName('机器人配置 v2.yaml')).toBe(false);
    });

    it('扩展名白名单 + 无扩展名常见文件判文本', () => {
        expect(looksLikeText('bot_config.toml')).toBe(true);
        expect(looksLikeText('napcat.log')).toBe(true);
        expect(looksLikeText('Dockerfile')).toBe(true);
        expect(looksLikeText('README')).toBe(true);
        expect(looksLikeText('.env.prod')).toBe(true);
        expect(looksLikeText('WEBAPP.LOCK')).toBe(true);
        expect(looksLikeText('cover.png')).toBe(false);
        expect(looksLikeText('node.exe')).toBe(false);
        expect(looksLikeText('Makefile.am')).toBe(false);
    });

    it('编辑器语法按扩展名分流', () => {
        expect(editorSyntaxOf('onebot11_10001.JSON')).toBe('json');
        expect(editorSyntaxOf('settings.jsonc')).toBe('json');
        expect(editorSyntaxOf('bot_config.toml')).toBe('toml');
        expect(editorSyntaxOf('.env')).toBe('dot_env');
        expect(editorSyntaxOf('app/.env.production')).toBe('plain');
        expect(editorSyntaxOf('docker-compose.yaml')).toBe('plain');
        expect(editorSyntaxOf('run.sh')).toBe('plain');
    });
});

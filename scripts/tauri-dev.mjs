import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir, tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

function desktopString(value) {
    return value
        .replace(/\\/g, '\\\\')
        .replace(/\n/g, '\\n')
        .replace(/\r/g, '\\r')
        .replace(/\t/g, '\\t')
        .replace(/^ +| +$/g, (spaces) => '\\s'.repeat(spaces.length));
}

function desktopArgument(value) {
    // Exec 先按桌面文件字符串解码，再按命令参数解码；百分号另按字段码转义。
    return `"${value.replace(/[\\`"$]/g, '\\$&').replace(/%/g, '%%')}"`;
}

export function prepareLinuxDesktop({
    env = process.env,
    workspace = root,
    executable = process.execPath,
    home = homedir(),
} = {}) {
    const config = JSON.parse(
        readFileSync(path.join(workspace, 'src-tauri/tauri.conf.json'), 'utf8'),
    );
    const id = `${config.identifier}.Development`;
    const applications = path.join(
        env.XDG_DATA_HOME || path.join(home, '.local/share'),
        'applications',
    );
    const desktopFile = path.join(applications, `${id}.desktop`);
    const command = [executable, path.join(workspace, 'scripts/tauri-dev.mjs')]
        .map(desktopArgument)
        .join(' ');
    const contents = [
        '[Desktop Entry]',
        'Type=Application',
        `Name=${desktopString(`${config.productName} (Development)`)}`,
        `Exec=${desktopString(command)}`,
        `Path=${desktopString(workspace)}`,
        `Icon=${desktopString(path.join(workspace, 'src-tauri/icons/256x256.png'))}`,
        `StartupWMClass=${id}`,
        'Terminal=false',
        'NoDisplay=true',
        '',
    ].join('\n');
    mkdirSync(applications, { recursive: true });
    writeFileSync(desktopFile, contents);
    return { id, desktopFile };
}

export function devEnvironment(env = process.env, platform = process.platform, workspace = root) {
    if (platform !== 'win32' || env.CARGO_TARGET_DIR) return { ...env };
    // 工作区可能继承 Low 完整性；可执行产物需放到普通用户目录中。
    const id = createHash('sha256')
        .update(path.win32.resolve(workspace).toLowerCase())
        .digest('hex')
        .slice(0, 16);
    return {
        ...env,
        CARGO_TARGET_DIR: path.win32.join(
            env.LOCALAPPDATA || tmpdir(),
            'NapCatQQ Desktop',
            'cargo-targets',
            id,
        ),
    };
}

function main() {
    const env = devEnvironment();
    if (env.CARGO_TARGET_DIR !== process.env.CARGO_TARGET_DIR) {
        console.log(`[dev] Cargo target: ${env.CARGO_TARGET_DIR}`);
        mkdirSync(env.CARGO_TARGET_DIR, { recursive: true });
    }
    if (env.CARGO_TARGET_DIR) process.env.CARGO_TARGET_DIR = env.CARGO_TARGET_DIR;
    const args = ['dev'];
    if (process.platform === 'linux') {
        const { id } = prepareLinuxDesktop();
        process.env.NCD_DEV_DESKTOP_ID = id;
        // 开发窗使用独立 Desktop ID，正式包仍由 identifier 提供 GTK app ID。
        args.push('--config', JSON.stringify({ app: { enableGTKAppId: false } }));
    }
    process.chdir(root);
    const cli = fileURLToPath(new URL('../node_modules/@tauri-apps/cli/tauri.js', import.meta.url));
    process.argv.splice(1, 1, cli, ...args);
    // 在同一进程加载原 CLI，沿用其信号处理和开发子进程收尾。
    createRequire(import.meta.url)(cli);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();

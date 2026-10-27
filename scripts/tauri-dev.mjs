import { createHash } from 'node:crypto';
import { mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

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
    process.chdir(root);
    const cli = fileURLToPath(new URL('../node_modules/@tauri-apps/cli/tauri.js', import.meta.url));
    process.argv.splice(1, 1, cli, 'dev');
    // 在同一进程加载原 CLI，沿用其信号处理和开发子进程收尾。
    createRequire(import.meta.url)(cli);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();

// 将仓库路径转为绝对路径，避免 Vitest 再次按 src-ui 根拼接而漏跑。
import { spawnSync } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const uiRoot = resolve(repoRoot, 'src-ui');
const args = process.argv.slice(2);
const separator = args.indexOf('--');
const sources = separator < 0 ? args : args.slice(0, separator);
const options = separator < 0 ? [] : args.slice(separator + 1);

try {
    if (!sources.length)
        throw new Error('Usage: pnpm run test:related <source files> [-- <Vitest options>]');
    const files = sources.map((source) => {
        const fromRepo = resolve(repoRoot, source);
        const file = existsSync(fromRepo) ? fromRepo : resolve(uiRoot, source);
        if (!existsSync(file) || !statSync(file).isFile())
            throw new Error(`Source file not found: ${source}`);
        return file;
    });
    const result = spawnSync(
        process.execPath,
        [
            fileURLToPath(import.meta.resolve('vitest/vitest.mjs')),
            'related',
            '--run',
            '--config',
            resolve(uiRoot, 'vitest.config.ts'),
            ...files,
            ...options,
        ],
        { cwd: repoRoot, stdio: 'inherit' },
    );
    if (result.error) throw result.error;
    process.exitCode = result.status ?? 1;
} catch (error) {
    console.error(error.message);
    process.exitCode = 1;
}

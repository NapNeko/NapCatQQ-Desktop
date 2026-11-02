// 发版缓存只保留第三方依赖，排除可能携带编译期凭据的本仓库产物。
import { existsSync, lstatSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const workspaceArtifact = /^(?:lib)?ncd[-_]|^(?:lib)?NapCatQQ[-_]Desktop|^proxy_constants\.rs$|^(?:bundle|wix|incremental)$/;

export function cleanReleaseCache(root, { check = false } = {}) {
    const target = join(realpathSync(root), 'target');
    if (!existsSync(target)) return 0;
    if (lstatSync(target).isSymbolicLink()) throw new Error('Refusing a linked target directory');

    function collect(directory) {
        return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
            const path = join(directory, entry.name);
            if (entry.isSymbolicLink()) throw new Error(`Refusing linked cache entry: ${path}`);
            if (workspaceArtifact.test(entry.name)) return [path];
            return entry.isDirectory() ? collect(path) : [];
        });
    }

    // 先检查整棵目录，遇到链接或异常时不开始删除。
    const artifacts = collect(target);
    if (check && artifacts.length) {
        throw new Error(`Dependency cache contains workspace artifacts: ${artifacts[0]}`);
    }
    for (const path of artifacts) rmSync(path, { recursive: true });
    return artifacts.length;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const command = process.argv[2];
    if (command !== 'check' && command !== 'clean') throw new Error('Expected check or clean');
    const removed = cleanReleaseCache(process.cwd(), { check: command === 'check' });
    console.log(`Release dependency cache ${command}: ${removed} workspace entries`);
}

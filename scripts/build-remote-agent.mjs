import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, createReadStream, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

if (process.platform !== 'win32') process.exit(0);

const root = fileURLToPath(new URL('../', import.meta.url));
const release = process.argv.includes('--release');
const args = ['build', '-p', 'ncd-agent'];
if (release) args.push('--release');
const compiler = spawnSync('rustc', ['-vV'], { encoding: 'utf8' });
if (compiler.error) throw compiler.error;
if (compiler.status !== 0) process.exit(compiler.status ?? 1);
const triple =
    process.env.TAURI_ENV_TARGET_TRIPLE ||
    process.env.CARGO_BUILD_TARGET ||
    compiler.stdout.match(/^host: (.+)$/m)?.[1].trim();
if (!triple?.endsWith('-pc-windows-msvc')) {
    throw new Error(`Remote Agent requires a Windows MSVC target: ${triple}`);
}
args.push('--target', triple);
// The target must bootstrap before its VC++ component can be installed.
const env = { ...process.env, CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL ?? '0' };
if (env.CARGO_ENCODED_RUSTFLAGS !== undefined) {
    env.CARGO_ENCODED_RUSTFLAGS = [env.CARGO_ENCODED_RUSTFLAGS, '-C', 'target-feature=+crt-static']
        .filter(Boolean)
        .join('\x1f');
} else {
    env.RUSTFLAGS = `${env.RUSTFLAGS ?? ''} -C target-feature=+crt-static`.trim();
}
const built = spawnSync('cargo', args, { cwd: root, env, stdio: 'inherit' });
if (built.error) throw built.error;
if (built.status !== 0) process.exit(built.status ?? 1);

const target = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target');
const profile = release ? 'release' : 'debug';
const source = path.join(target, triple, profile, 'ncd-agent.exe');
const pe = readFileSync(source);
const header = pe.readUInt32LE(0x3c);
if (pe.toString('ascii', header, header + 4) !== 'PE\0\0') {
    throw new Error('Agent build did not produce a PE executable');
}
const optional = header + 24;
const pe64 = pe.readUInt16LE(optional) === 0x20b;
const importsRva = pe.readUInt32LE(optional + (pe64 ? 120 : 104));
const sections = optional + pe.readUInt16LE(header + 20);
function fileOffset(rva) {
    for (let index = 0; index < pe.readUInt16LE(header + 6); index++) {
        const entry = sections + index * 40;
        const address = pe.readUInt32LE(entry + 12);
        const size = Math.max(pe.readUInt32LE(entry + 8), pe.readUInt32LE(entry + 16));
        if (rva >= address && rva < address + size) {
            return pe.readUInt32LE(entry + 20) + rva - address;
        }
    }
    throw new Error(`Invalid Agent PE address: ${rva}`);
}
if (importsRva !== 0) {
    let descriptor = fileOffset(importsRva);
    while (pe.readUInt32LE(descriptor + 12) !== 0) {
        const start = fileOffset(pe.readUInt32LE(descriptor + 12));
        const dependency = pe.toString('ascii', start, pe.indexOf(0, start));
        if (/^(vcruntime|msvcp|concrt)\d.*\.dll$/i.test(dependency)) {
            throw new Error(`Agent still depends on an unbundled VC++ runtime: ${dependency}`);
        }
        descriptor += 20;
    }
}
const resources = path.join(root, 'src-tauri', 'resources', 'remote-agent');
mkdirSync(resources, { recursive: true });
const destination = path.join(resources, 'ncd-agent.exe');
async function digest(file) {
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(file)) hash.update(chunk);
    return hash.digest('hex');
}
if (!existsSync(destination) || (await digest(source)) !== (await digest(destination))) {
    copyFileSync(source, destination);
}

// 内容一致时恢复输入时间戳，避免 checkout 让 Cargo 缓存整体失效。
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import {
    appendFileSync,
    copyFileSync,
    existsSync,
    lstatSync,
    mkdirSync,
    readdirSync,
    readFileSync,
    statSync,
    utimesSync,
    writeFileSync,
} from 'node:fs';
import { dirname, posix, resolve } from 'node:path';
import { arch, homedir, platform } from 'node:os';
import { fileURLToPath } from 'node:url';

const generatedProxy = 'crates/ncd-network/src/proxy_constants.rs';
const proxyTemplate = 'crates/ncd-network/src/proxy_constants.template.rs';
const digest = (data) => createHash('sha256').update(data).digest('hex');

function trackedFiles(root) {
    return execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' })
        .split('\0')
        .filter(Boolean)
        .sort();
}

function inputFiles(root) {
    const files = trackedFiles(root);
    if (existsSync(resolve(root, generatedProxy))) files.push(generatedProxy);
    const dotenv = resolve(root, '.env');
    if (existsSync(dotenv) && statSync(dotenv).size === 0) files.push('.env');
    return [...new Set(files)].filter((file) => lstatSync(resolve(root, file)).isFile());
}

function inputDirectories(files) {
    const directories = new Set();
    for (const file of Object.keys(files)) {
        for (let dir = posix.dirname(file); dir !== '.'; dir = posix.dirname(dir))
            directories.add(dir);
    }
    return directories;
}

function directoryHash(files, directory) {
    return digest(
        JSON.stringify(
            Object.entries(files)
                .filter(([file]) => file.startsWith(`${directory}/`))
                .map(([file, entry]) => [file, entry.hash])
                .sort(([a], [b]) => a.localeCompare(b)),
        ),
    );
}

export function sourceKey(root) {
    const source = createHash('sha256');
    for (const file of trackedFiles(root)) {
        source
            .update(file)
            .update('\0')
            .update(digest(readFileSync(resolve(root, file))))
            .update('\0');
    }
    return source.digest('hex');
}

export function cacheKeys(root) {
    const buildEnv = Object.fromEntries(
        Object.entries(process.env)
            .filter(([key]) =>
                /^(RUSTFLAGS$|CARGO_ENCODED_RUSTFLAGS$|CARGO_PROFILE_|CARGO_TARGET_|CC$|CXX$|CFLAGS$|CXXFLAGS$|ImageOS$|ImageVersion$)/.test(
                    key,
                ),
            )
            .sort(([a], [b]) => a.localeCompare(b)),
    );
    return {
        'source-key': sourceKey(root),
        'environment-key': digest(
            JSON.stringify({
                rustc: execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).trim(),
                cargo: execFileSync('cargo', ['-V'], { encoding: 'utf8' }).trim(),
                platform: platform(),
                arch: arch(),
                buildEnv,
            }),
        ),
        'cargo-home': process.env.CARGO_HOME || resolve(homedir(), '.cargo'),
    };
}

export function captureInputs(root, manifest) {
    const files = Object.fromEntries(
        inputFiles(root).map((file) => {
            const path = resolve(root, file);
            return [
                file,
                { hash: digest(readFileSync(path)), mtimeMs: Math.floor(statSync(path).mtimeMs) },
            ];
        }),
    );
    const directories = Object.fromEntries(
        [...inputDirectories(files)].map((dir) => {
            const path = resolve(root, dir);
            return [
                dir,
                {
                    hash: directoryHash(files, dir),
                    children: readdirSync(path).sort(),
                    mtimeMs: Math.floor(statSync(path).mtimeMs),
                },
            ];
        }),
    );
    mkdirSync(dirname(manifest), { recursive: true });
    writeFileSync(manifest, JSON.stringify({ version: 1, files, directories }));
    return Object.keys(files).length;
}

export function restoreInputs(root, manifest, onChanged = () => {}) {
    // build.rs 在源码树生成此文件；缓存命中时仍须让 dep-info 找到它。
    const proxy = resolve(root, generatedProxy);
    const template = resolve(root, proxyTemplate);
    if (!existsSync(proxy) && existsSync(template)) copyFileSync(template, proxy);
    // 缺失的 rerun-if-changed 输入会让 build.rs 每轮都运行；只补空文件。
    const dotenv = resolve(root, '.env');
    if (!existsSync(dotenv) && existsSync(template)) writeFileSync(dotenv, '');
    if (!existsSync(manifest)) return { restored: 0, changed: 0 };

    let snapshot;
    try {
        snapshot = JSON.parse(readFileSync(manifest, 'utf8'));
    } catch {
        console.warn('Ignoring unreadable Cargo input timestamps');
        return { restored: 0, changed: 0 };
    }
    if (
        !snapshot ||
        snapshot.version !== 1 ||
        !snapshot.files ||
        typeof snapshot.files !== 'object'
    )
        return { restored: 0, changed: 0 };

    let restored = 0;
    let changed = 0;
    const current = {};
    for (const file of inputFiles(root)) {
        const path = resolve(root, file);
        const hash = digest(readFileSync(path));
        current[file] = { hash };
        const previous = snapshot.files[file];
        if (
            !previous ||
            !Number.isFinite(previous.mtimeMs) ||
            previous.mtimeMs < 0 ||
            previous.mtimeMs > Date.now()
        )
            continue;
        if (hash !== previous.hash) {
            changed++;
            onChanged(file);
            continue;
        }
        utimesSync(path, statSync(path).atime, previous.mtimeMs / 1000);
        restored++;
    }
    // rerun-if-changed 也可指向目录；只有成员与内容都一致才恢复目录时间。
    for (const dir of inputDirectories(current)) {
        const previous = snapshot.directories?.[dir];
        if (
            !previous ||
            !Number.isFinite(previous.mtimeMs) ||
            previous.mtimeMs < 0 ||
            previous.mtimeMs > Date.now()
        )
            continue;
        const path = resolve(root, dir);
        if (
            directoryHash(current, dir) !== previous.hash ||
            JSON.stringify(readdirSync(path).sort()) !== JSON.stringify(previous.children)
        )
            continue;
        utimesSync(path, statSync(path).atime, previous.mtimeMs / 1000);
    }
    return { restored, changed };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const root = process.cwd();
    const manifest = resolve(root, 'target/ci-source-mtimes.json');
    switch (process.argv[2]) {
        case 'keys': {
            const keys = cacheKeys(root);
            if (process.env.GITHUB_OUTPUT)
                appendFileSync(
                    process.env.GITHUB_OUTPUT,
                    Object.entries(keys)
                        .map(([key, value]) => `${key}=${value}\n`)
                        .join(''),
                );
            console.log(
                `Cargo source=${keys['source-key']} environment=${keys['environment-key']}`,
            );
            break;
        }
        case 'restore':
            console.log(
                'Cargo input timestamps:',
                restoreInputs(root, manifest, (file) =>
                    console.log(`Cargo changed input: ${file}`),
                ),
            );
            break;
        case 'capture':
            console.log(`Recorded ${captureInputs(root, manifest)} Cargo input timestamps`);
            break;
        default:
            throw new Error('Expected keys, restore, or capture');
    }
}

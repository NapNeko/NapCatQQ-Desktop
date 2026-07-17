#!/usr/bin/env node
/**
 * Desktop 发版入口（对齐旧版 release.py：本地真相源，默认不 push）。
 *
 * 一键（推荐）:
 *   pnpm run release -- 3.1.2
 *   pnpm run release -- 3.1.2 --push
 *   pnpm run release -- 3.1.2 --yes --push
 *
 * 分步:
 *   pnpm run release:bump -- 3.1.2
 *   pnpm run release:prepare -- 3.1.2 [--tag]
 */

import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');

const PACKAGE_JSON = path.join(ROOT, 'package.json');
const TAURI_CONF = path.join(ROOT, 'src-tauri', 'tauri.conf.json');
const APP_META = path.join(ROOT, 'src-ui', 'core', 'domain', 'app-meta.ts');
const RELEASES_DIR = path.join(ROOT, 'docs', 'releases');

const VERSION_FILES = [
    'package.json',
    'src-tauri/tauri.conf.json',
    'src-ui/core/domain/app-meta.ts',
];

async function main() {
    const args = parseArgs(process.argv.slice(2));
    if (!args.cmd || args.help) {
        printHelp();
        process.exit(args.help ? 0 : 1);
    }

    if (args.cmd === 'bump') {
        cmdBump(requireVersion(args.version));
        return;
    }
    if (args.cmd === 'prepare') {
        cmdPrepare(requireVersion(args.version), {
            allowDirty: !!args.allowDirty,
            tag: !!args.tag,
        });
        return;
    }
    if (args.cmd === 'ship' || args.cmd === 'release') {
        await cmdShip(requireVersion(args.version), {
            push: !!args.push,
            yes: !!args.yes,
        });
        return;
    }

    // pnpm run release -- 3.1.2
    if (/^v?\d+\.\d+\.\d+$/i.test(args.cmd)) {
        await cmdShip(requireVersion(args.cmd), {
            push: !!args.push,
            yes: !!args.yes,
        });
        return;
    }

    console.error(`未知命令: ${args.cmd}`);
    printHelp();
    process.exit(1);
}

async function cmdShip(versionPlain, { push, yes }) {
    const tagName = `v${versionPlain}`;
    const curatedRel = `docs/releases/v${versionPlain}.md`;
    const curatedPath = path.join(ROOT, curatedRel);
    // 允许「先 draft 再 release」：仅本版策展 + 版本三处可脏，其它改动仍拦截
    const allowedDirty = new Set([...VERSION_FILES, curatedRel]);

    console.log(`[ship] NapCatQQ Desktop ${versionPlain} → ${tagName}`);
    console.log(`[ship] push=${push ? 'yes' : 'no（默认）'} yes=${yes ? 'yes' : 'no'}`);
    console.log('');

    const dirtyMap = gitStatusMap();
    const foreign = [...dirtyMap.keys()].filter((f) => !allowedDirty.has(f));
    if (foreign.length) {
        fail(
            `工作区有与本版发版无关的改动，请先提交或 stash：\n${foreign.map((f) => `  ${dirtyMap.get(f)} ${f}`).join('\n')}`,
        );
    }
    if (dirtyMap.size === 0) {
        console.log('[ok] 工作区干净');
    } else {
        console.log(
            `[ok] 仅本版发版相关改动（${[...dirtyMap.keys()].join(', ')}），允许继续`,
        );
    }

    if (gitTagExists(tagName)) fail(`本地已存在 tag ${tagName}`);
    if (remoteTagExists(tagName)) fail(`远端已存在 tag ${tagName}（origin）`);
    console.log(`[ok] 无 tag ${tagName}（本地/origin）`);

    let notesCreated = false;
    if (!fs.existsSync(curatedPath)) {
        console.log(`[info] 无 ${curatedRel}，生成草稿（优先 .env 模型，失败则规则归类）…`);
        runNode([
            'scripts/release-notes.mjs',
            'draft',
            '--kind',
            'desktop',
            '--version',
            versionPlain,
        ]);
        notesCreated = true;
    }
    if (!fs.existsSync(curatedPath)) fail(`策展文件仍不存在: ${curatedRel}`);
    const notesBody = fs.readFileSync(curatedPath, 'utf8');
    if (notesBody.trim().length < 40) fail(`策展文件过短: ${curatedRel}`);
    const looksAuto =
        notesCreated || /请删改本文件后再|正文为自动草稿|自动归类/.test(notesBody);
    console.log(`[ok] 策展: ${curatedRel}${looksAuto ? '（仍偏自动草稿，建议先改）' : ''}`);

    const previewOut = path.join(ROOT, 'tmp', `.release-notes-${versionPlain}.md`);
    fs.mkdirSync(path.dirname(previewOut), { recursive: true });
    runNode([
        'scripts/release-notes.mjs',
        'render',
        '--kind',
        'desktop',
        '--version',
        versionPlain,
        '--out',
        previewOut,
    ]);
    const preview = fs.readFileSync(previewOut, 'utf8');
    console.log('');
    console.log('---- Release 正文预览（前 40 行）----');
    console.log(preview.split(/\r?\n/).slice(0, 40).join('\n'));
    console.log(`---- 全文: ${path.relative(ROOT, previewOut).replace(/\\/g, '/')} ----`);
    console.log('');

    if (!yes) {
        const ok = await confirm(
            looksAuto
                ? `策展可能仍是自动草稿。确认 commit + tag ${tagName}${push ? ' + push' : ''}？ [y/N] `
                : `确认发布 ${tagName}（commit + 本地 tag${push ? ' + push' : ''}）？ [y/N] `,
        );
        if (!ok) {
            console.error('[err] 已取消，未改版本、未提交、未打 tag');
            process.exit(1);
        }
    }

    const before = readVersions();
    writeVersions(versionPlain);
    assertVersionsEqual(readVersions(), versionPlain);
    console.log(`[ok] 版本 ${before.pkg} → ${versionPlain}`);

    runNode([
        'scripts/release-notes.mjs',
        'render',
        '--kind',
        'desktop',
        '--version',
        versionPlain,
        '--out',
        previewOut,
    ]);
    const finalPreview = fs.readFileSync(previewOut, 'utf8');
    if (finalPreview.includes('正文为自动草稿')) {
        fail('render 仍为自动草稿，请检查 docs/releases 后重试');
    }

    const toAdd = [...VERSION_FILES, curatedRel];
    for (const f of toAdd) {
        if (!fs.existsSync(path.join(ROOT, f))) fail(`缺少文件: ${f}`);
    }
    git(['add', ...toAdd]);
    const staged = git(['diff', '--cached', '--name-only']);
    if (!staged.trim()) fail('没有可提交的发版改动');
    const allowed = new Set(toAdd.map((p) => p.replace(/\\/g, '/')));
    for (const f of staged.split(/\r?\n/).map((s) => s.trim()).filter(Boolean)) {
        const norm = f.replace(/\\/g, '/');
        if (!allowed.has(norm)) {
            git(['reset', 'HEAD', '--', f]);
            fail(`拒绝提交无关文件: ${f}（已 unstage）`);
        }
    }

    git([
        'commit',
        '-m',
        `chore(release): 发布 ${tagName}`,
        '-m',
        `版本元数据对齐 ${versionPlain}；策展 docs/releases/v${versionPlain}.md`,
    ]);
    const sha = git(['rev-parse', '--short', 'HEAD']);
    console.log(`[ok] commit ${sha}`);

    git(['tag', '-a', tagName, '-m', `NapCatQQ Desktop ${versionPlain}`]);
    console.log(`[ok] 本地 tag ${tagName}`);

    if (push) {
        console.log(`[info] push origin HEAD && push origin ${tagName}`);
        git(['push', 'origin', 'HEAD'], { inherit: true });
        git(['push', 'origin', tagName], { inherit: true });
        console.log('[ok] 已推送。Release MSI 应由 tag 触发');
        console.log('     https://github.com/NapNeko/NapCatQQ-Desktop/actions');
    } else {
        console.log('');
        console.log('[ok] 本地发版完成（未 push，对齐旧版默认）');
        console.log(`     git push origin HEAD && git push origin ${tagName}`);
    }
}

function cmdBump(versionPlain) {
    const tag = `v${versionPlain}`;
    const before = readVersions();
    writeVersions(versionPlain);
    assertVersionsEqual(readVersions(), versionPlain);
    if (gitTagExists(tag)) console.warn(`[warn] 本地已有 tag ${tag}`);
    console.log(`[ok] 版本已写入 ${versionPlain}`);
    console.log(`     package.json:      ${before.pkg} → ${readVersions().pkg}`);
    console.log(`     tauri.conf.json:   ${before.tauri} → ${readVersions().tauri}`);
    console.log(`     APP_VERSION:       ${before.app} → ${readVersions().app}`);
    console.log(`     APP_VERSION_LABEL: ${before.label} → ${readVersions().label}`);
    console.log('');
    console.log(`一键发版: pnpm run release -- ${versionPlain}`);
}

function cmdPrepare(versionPlain, { allowDirty, tag }) {
    const tagName = `v${versionPlain}`;
    const curated = path.join(RELEASES_DIR, `v${versionPlain}.md`);
    const errors = [];
    const warns = [];

    try {
        assertVersionsEqual(readVersions(), versionPlain);
        console.log(`[ok] 版本三处一致: ${versionPlain}`);
    } catch (e) {
        errors.push(String(e.message || e));
    }

    if (!fs.existsSync(curated)) errors.push(`缺少策展: docs/releases/v${versionPlain}.md`);
    else console.log('[ok] 策展文件存在');

    if (gitTagExists(tagName)) errors.push(`本地已存在 tag ${tagName}`);
    else console.log(`[ok] 本地无 tag ${tagName}`);

    const dirty = gitStatusPorcelain();
    if (dirty) {
        if (allowDirty) {
            warns.push(`工作区不干净:\n${dirty}`);
            console.log('[warn] 工作区不干净（已允许）');
        } else errors.push(`工作区不干净:\n${dirty}`);
    } else {
        console.log('[ok] 工作区干净');
        const headPkg = readFileAtHead('package.json');
        if (headPkg) {
            try {
                const hv = JSON.parse(headPkg).version;
                if (hv !== versionPlain) errors.push(`HEAD package.json 仍是 ${hv}`);
                else console.log(`[ok] HEAD 已包含版本 ${versionPlain}`);
            } catch {
                /* ignore */
            }
        }
        if (fs.existsSync(curated) && readFileAtHead(`docs/releases/v${versionPlain}.md`) == null) {
            errors.push(`HEAD 尚无 docs/releases/v${versionPlain}.md`);
        }
    }

    try {
        const out = path.join(ROOT, 'tmp', `.release-notes-${versionPlain}.md`);
        fs.mkdirSync(path.dirname(out), { recursive: true });
        runNode([
            'scripts/release-notes.mjs',
            'render',
            '--kind',
            'desktop',
            '--version',
            versionPlain,
            '--out',
            out,
        ]);
        const text = fs.readFileSync(out, 'utf8');
        if (text.includes('正文为自动草稿')) errors.push('render 仍是自动草稿');
        else console.log('[ok] notes render 策展正文');
    } catch (e) {
        errors.push(`render 失败: ${e.stderr || e.message || e}`);
    }

    for (const w of warns) console.warn(`[warn] ${w}`);
    if (errors.length) {
        for (const err of errors) console.error(`[err] ${err}`);
        fail('prepare 未通过');
    }

    console.log(`[ok] prepare 通过 → ${tagName}`);
    if (tag) {
        if (dirty) fail('--tag 要求工作区干净');
        git(['tag', '-a', tagName, '-m', `NapCatQQ Desktop ${versionPlain}`], { inherit: true });
        console.log(`[ok] 已创建本地 tag ${tagName}（未 push）`);
    }
}

function writeVersions(versionPlain) {
    const pkg = JSON.parse(fs.readFileSync(PACKAGE_JSON, 'utf8'));
    pkg.version = versionPlain;
    fs.writeFileSync(PACKAGE_JSON, `${JSON.stringify(pkg, null, 2)}\n`, 'utf8');

    let tauri = fs.readFileSync(TAURI_CONF, 'utf8');
    const tauriNext = tauri.replace(/^(\s*"version"\s*:\s*)"[^"]*"/m, `$1"${versionPlain}"`);
    if (tauriNext === tauri) {
        const cur = JSON.parse(tauri).version;
        if (cur !== versionPlain) fail(`无法替换 tauri.conf.json version（当前 ${cur}）`);
    }
    fs.writeFileSync(TAURI_CONF, tauriNext, 'utf8');

    let meta = fs.readFileSync(APP_META, 'utf8');
    meta = meta.replace(
        /export const APP_VERSION_LABEL = 'v[^']*'/,
        `export const APP_VERSION_LABEL = 'v${versionPlain}'`,
    );
    meta = meta.replace(
        /export const APP_VERSION = '[^']*'/,
        `export const APP_VERSION = '${versionPlain}'`,
    );
    if (!meta.includes(`APP_VERSION = '${versionPlain}'`)) fail('无法更新 APP_VERSION');
    if (!meta.includes(`APP_VERSION_LABEL = 'v${versionPlain}'`)) fail('无法更新 APP_VERSION_LABEL');
    fs.writeFileSync(APP_META, meta, 'utf8');
}

function readVersions() {
    const pkg = JSON.parse(fs.readFileSync(PACKAGE_JSON, 'utf8')).version;
    const tauri = JSON.parse(fs.readFileSync(TAURI_CONF, 'utf8')).version;
    const meta = fs.readFileSync(APP_META, 'utf8');
    const app = (meta.match(/export const APP_VERSION = '([^']+)'/) || [])[1] || '';
    const label = (meta.match(/export const APP_VERSION_LABEL = '(v[^']+)'/) || [])[1] || '';
    return { pkg, tauri, app, label };
}

function assertVersionsEqual(v, versionPlain) {
    const mismatches = [];
    if (v.pkg !== versionPlain) mismatches.push(`package.json=${v.pkg}`);
    if (v.tauri !== versionPlain) mismatches.push(`tauri.conf.json=${v.tauri}`);
    if (v.app !== versionPlain) mismatches.push(`APP_VERSION=${v.app}`);
    if (v.label !== `v${versionPlain}`) mismatches.push(`APP_VERSION_LABEL=${v.label}`);
    if (mismatches.length) {
        throw new Error(`版本不一致（期望 ${versionPlain}）: ${mismatches.join(', ')}`);
    }
}

function requireVersion(raw) {
    if (!raw || !String(raw).trim()) fail('请提供版本号，例如 3.1.2');
    const s = String(raw).trim().replace(/^v/i, '');
    if (!/^\d+\.\d+\.\d+$/.test(s)) fail(`版本号格式应为 X.Y.Z，收到: ${raw}`);
    return s;
}

function git(args, { inherit = false } = {}) {
    if (inherit) {
        const r = spawnSync('git', args, { cwd: ROOT, stdio: 'inherit', encoding: 'utf8' });
        if (r.status !== 0) fail(`git ${args.join(' ')} 失败`);
        return '';
    }
    try {
        return execFileSync('git', args, {
            cwd: ROOT,
            encoding: 'utf8',
            stdio: ['ignore', 'pipe', 'pipe'],
        }).trim();
    } catch (e) {
        const err = (e.stderr || e.stdout || e.message || '').toString().trim();
        fail(`git ${args.join(' ')} 失败${err ? `: ${err}` : ''}`);
    }
}

function runNode(args) {
    const r = spawnSync(process.execPath, args, {
        cwd: ROOT,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    if (r.status !== 0) fail(`node ${args.join(' ')} 失败\n${r.stderr || r.stdout || ''}`);
    return (r.stdout || '').trim();
}

function gitTagExists(tag) {
    try {
        execFileSync('git', ['rev-parse', '-q', '--verify', `refs/tags/${tag}`], {
            cwd: ROOT,
            stdio: 'ignore',
        });
        return true;
    } catch {
        return false;
    }
}

function remoteTagExists(tag) {
    try {
        const out = execFileSync('git', ['ls-remote', '--tags', 'origin', tag], {
            cwd: ROOT,
            encoding: 'utf8',
            stdio: ['ignore', 'pipe', 'pipe'],
        }).trim();
        return out.length > 0;
    } catch {
        return false;
    }
}

function gitStatusPorcelain() {
    try {
        return execFileSync('git', ['status', '--porcelain'], {
            cwd: ROOT,
            encoding: 'utf8',
        }).trim();
    } catch {
        return '';
    }
}

/** path → XY status；路径统一正斜杠 */
function gitStatusMap() {
    const raw = gitStatusPorcelain();
    const map = new Map();
    if (!raw) return map;
    for (const line of raw.split(/\r?\n/)) {
        if (!line.trim()) continue;
        // porcelain v1: 两位状态 + 一个空格 + path（rename 为 "old -> new"）
        const m = /^(?<xy>[\sMADRCU?!]{2}) (?<path>.+)$/.exec(line);
        if (!m) continue;
        const xy = m.groups.xy;
        let file = m.groups.path.trim();
        if (file.includes(' -> ')) file = file.split(' -> ').pop().trim();
        if (
            (file.startsWith('"') && file.endsWith('"')) ||
            (file.startsWith("'") && file.endsWith("'"))
        ) {
            file = file.slice(1, -1);
        }
        map.set(file.replace(/\\/g, '/'), xy);
    }
    return map;
}

function readFileAtHead(relPath) {
    try {
        return execFileSync('git', ['show', `HEAD:${relPath.replace(/\\/g, '/')}`], {
            cwd: ROOT,
            encoding: 'utf8',
            stdio: ['ignore', 'pipe', 'pipe'],
        });
    } catch {
        return null;
    }
}

function confirm(question) {
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
    return new Promise((resolve) => {
        rl.question(question, (answer) => {
            rl.close();
            const a = String(answer || '')
                .trim()
                .toLowerCase();
            resolve(a === 'y' || a === 'yes');
        });
    });
}

function fail(msg) {
    console.error(`[err] ${msg}`);
    process.exit(1);
}

function parseArgs(argv) {
    const out = {
        cmd: '',
        version: '',
        tag: false,
        allowDirty: false,
        push: false,
        yes: false,
        help: false,
    };
    if (!argv.length) return out;
    // 允许 `release --help` / `release 3.1.2 --push`
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--help' || a === '-h') out.help = true;
        else if (a === '--tag') out.tag = true;
        else if (a === '--allow-dirty') out.allowDirty = true;
        else if (a === '--push') out.push = true;
        else if (a === '--yes' || a === '-y') out.yes = true;
        else if (a === '--version' || a === '-v') out.version = argv[++i];
        else if (a.startsWith('--version=')) out.version = a.slice('--version='.length);
        else if (!a.startsWith('-')) {
            if (!out.cmd) out.cmd = a;
            else if (!out.version) out.version = a;
        }
    }
    return out;
}

function printHelp() {
    console.log(`release.mjs — Desktop 一键发版（对齐旧版 release.py）

用法:
  pnpm run release -- <X.Y.Z>              # commit + 本地 tag，不 push
  pnpm run release -- <X.Y.Z> --push       # 再推送分支与 tag → CI Release MSI
  pnpm run release -- <X.Y.Z> --yes --push # 跳过确认

分步:
  pnpm run release:bump -- <X.Y.Z>
  pnpm run release:prepare -- <X.Y.Z> [--tag]
  pnpm run release:notes:preview -- --version <X.Y.Z>

流程:
  1. 无无关脏文件（允许本版 docs/releases/vX.Y.Z.md 与版本三处未提交）
  2. tag 不存在；确保策展（没有则 draft）
  3. 预览正文 → 确认（--yes 跳过）
  4. 写版本三处 → 单 commit → annotated tag
  5. 可选 --push

推荐:
  pnpm run release:notes:draft -- --version X.Y.Z
  # 编辑 docs/releases/vX.Y.Z.md
  pnpm run release -- X.Y.Z
  pnpm run release -- X.Y.Z --push

注意:
  - 推送 tag 会触发正式 Release MSI
  - 默认不 push，与旧版 python release.py 一致
`);
}

main().catch((e) => {
    console.error(`[err] ${e.stack || e}`);
    process.exit(1);
});

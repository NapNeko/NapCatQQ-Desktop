#!/usr/bin/env node
/**
 * Desktop 发版辅助（对齐旧版 release.py 的闸门，默认不 push）。
 *
 *   node scripts/release.mjs bump 3.1.0
 *   node scripts/release.mjs prepare 3.1.0
 *   node scripts/release.mjs prepare 3.1.0 --tag          # 工作区干净时本地 annotated tag
 *   node scripts/release.mjs prepare 3.1.0 --allow-dirty  # 仅检查，允许未提交改动
 *
 * 不提供 --push：推送分支/tag 请人工执行，避免误触发正式 Release。
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');

const PACKAGE_JSON = path.join(ROOT, 'package.json');
const TAURI_CONF = path.join(ROOT, 'src-tauri', 'tauri.conf.json');
const APP_META = path.join(ROOT, 'src-ui', 'core', 'domain', 'app-meta.ts');
const RELEASES_DIR = path.join(ROOT, 'docs', 'releases');

function main() {
    const args = parseArgs(process.argv.slice(2));
    if (!args.cmd || args.help) {
        printHelp();
        process.exit(args.help ? 0 : 1);
    }

    if (args.cmd === 'bump') {
        const version = requireVersion(args.version);
        cmdBump(version);
        return;
    }
    if (args.cmd === 'prepare') {
        const version = requireVersion(args.version);
        cmdPrepare(version, {
            allowDirty: !!args.allowDirty,
            tag: !!args.tag,
        });
        return;
    }

    console.error(`未知命令: ${args.cmd}`);
    printHelp();
    process.exit(1);
}

function cmdBump(versionPlain) {
    // 对齐旧版 update_version.py：只写版本文件。tag 是否已存在由 prepare 拦截。
    const tag = `v${versionPlain}`;
    const before = readVersions();
    writeVersions(versionPlain);
    const after = readVersions();
    assertVersionsEqual(after, versionPlain);

    if (gitTagExists(tag)) {
        console.warn(`[warn] 本地已有 tag ${tag}；若非回写调试，prepare/发版前请确认不要重复发布`);
    }

    console.log(`[ok] 版本已写入 ${versionPlain}`);
    console.log(`     package.json:          ${before.pkg} → ${after.pkg}`);
    console.log(`     tauri.conf.json:       ${before.tauri} → ${after.tauri}`);
    console.log(`     APP_VERSION:           ${before.app} → ${after.app}`);
    console.log(`     APP_VERSION_LABEL:     ${before.label} → ${after.label}`);
    console.log('');
    console.log('下一步:');
    console.log(`  1. 确认 docs/releases/v${versionPlain}.md（pnpm run release:notes:preview -- --version ${versionPlain}）`);
    console.log('  2. git add 版本三处 + 策展文件后 commit');
    console.log(`  3. pnpm run release:prepare -- ${versionPlain}`);
    console.log(`  4. pnpm run release:prepare -- ${versionPlain} --tag   # 可选，仅本地 tag`);
    console.log(`  5. git push origin main && git push origin ${tag}   # 才会触发正式 Release`);
}

function cmdPrepare(versionPlain, { allowDirty, tag }) {
    const tagName = `v${versionPlain}`;
    const curated = path.join(RELEASES_DIR, `v${versionPlain}.md`);
    const errors = [];
    const warns = [];

    // 1) 版本三处
    try {
        const v = readVersions();
        assertVersionsEqual(v, versionPlain);
        console.log(`[ok] 版本三处一致: ${versionPlain}`);
    } catch (e) {
        errors.push(String(e.message || e));
    }

    // 2) 策展文件
    if (!fs.existsSync(curated)) {
        errors.push(`缺少策展文件: docs/releases/v${versionPlain}.md（先 draft/手写）`);
    } else {
        const body = fs.readFileSync(curated, 'utf8').trim();
        if (body.length < 40) {
            warns.push('策展文件过短，请确认已写用户向更新内容');
        }
        if (/正文为自动草稿|请删改本文件后再/.test(body)) {
            warns.push('策展文件仍像自动草稿提示，建议人工改写');
        }
        console.log(`[ok] 策展文件存在: docs/releases/v${versionPlain}.md`);
    }

    // 3) tag 不存在
    if (gitTagExists(tagName)) {
        errors.push(`本地已存在 tag ${tagName}`);
    } else {
        console.log(`[ok] 本地无 tag ${tagName}`);
    }

    // 4) 工作区
    const dirty = gitStatusPorcelain();
    if (dirty) {
        if (allowDirty) {
            warns.push(`工作区不干净（--allow-dirty）:\n${dirty}`);
            console.log('[warn] 工作区不干净（已允许）');
        } else {
            errors.push(
                `工作区不干净（对齐旧版 release.py）。提交后再 prepare，或加 --allow-dirty 仅检查。\n${dirty}`,
            );
        }
    } else {
        console.log('[ok] 工作区干净');
    }

    // 5) HEAD 是否已是目标版本（防「只改了文件没 commit 就 tag」在干净工作区下的误判：干净且版本对 = 已提交）
    if (!dirty) {
        const headPkg = readFileAtHead('package.json');
        if (headPkg) {
            try {
                const hv = JSON.parse(headPkg).version;
                if (hv !== versionPlain) {
                    errors.push(
                        `HEAD 上 package.json 仍是 ${hv}，与目标 ${versionPlain} 不一致（先 commit 版本 bump）`,
                    );
                } else {
                    console.log(`[ok] HEAD 已包含版本 ${versionPlain}`);
                }
            } catch {
                /* ignore */
            }
        }
        if (fs.existsSync(curated)) {
            const headCurated = readFileAtHead(`docs/releases/v${versionPlain}.md`);
            if (headCurated == null) {
                errors.push(
                    `HEAD 尚无 docs/releases/v${versionPlain}.md（策展必须进 tag 指向的 commit）`,
                );
            } else {
                console.log('[ok] HEAD 已包含策展文件');
            }
        }
    }

    // 6) 渲染预览抽检（不打印全文，只看是否策展）
    try {
        const rendered = execFileSync(
            process.execPath,
            [
                path.join(ROOT, 'scripts', 'release-notes.mjs'),
                'render',
                '--kind',
                'desktop',
                '--version',
                versionPlain,
                '--out',
                path.join(ROOT, 'tmp', `.release-notes-${versionPlain}.md`),
            ],
            { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
        ).trim();
        const outPath = rendered.split(/\r?\n/).filter(Boolean).pop();
        const text = fs.readFileSync(outPath, 'utf8');
        if (text.includes('正文为自动草稿')) {
            errors.push('render 结果仍是自动草稿（策展未生效或路径不对）');
        } else {
            console.log(`[ok] release notes render 为策展正文 → ${path.relative(ROOT, outPath)}`);
        }
    } catch (e) {
        errors.push(`release-notes render 失败: ${e.stderr || e.message || e}`);
    }

    for (const w of warns) console.warn(`[warn] ${w}`);

    if (errors.length) {
        console.error('');
        for (const err of errors) console.error(`[err] ${err}`);
        console.error('');
        console.error('[fail] prepare 未通过，未创建 tag、未推送');
        process.exit(1);
    }

    console.log('');
    console.log(`[ok] prepare 通过 → ${tagName}`);

    if (tag) {
        if (dirty) {
            fail('--tag 要求工作区干净；请先 commit');
        }
        execFileSync('git', ['tag', '-a', tagName, '-m', `NapCatQQ Desktop ${versionPlain}`], {
            cwd: ROOT,
            stdio: 'inherit',
        });
        console.log(`[ok] 已创建本地 tag ${tagName}（未 push）`);
        console.log(`推送正式发版: git push origin HEAD && git push origin ${tagName}`);
    } else {
        console.log('未打 tag。需要时:');
        console.log(`  pnpm run release:prepare -- ${versionPlain} --tag`);
        console.log(`  或: git tag -a ${tagName} -m "NapCatQQ Desktop ${versionPlain}"`);
        console.log(`推送才会触发 CI: git push origin HEAD && git push origin ${tagName}`);
    }
}

function writeVersions(versionPlain) {
    // package.json
    const pkg = JSON.parse(fs.readFileSync(PACKAGE_JSON, 'utf8'));
    pkg.version = versionPlain;
    fs.writeFileSync(PACKAGE_JSON, `${JSON.stringify(pkg, null, 2)}\n`, 'utf8');

    // tauri.conf.json 只替换顶层 "version" 行，避免整文件重排
    let tauri = fs.readFileSync(TAURI_CONF, 'utf8');
    const tauriNext = tauri.replace(
        /^(\s*"version"\s*:\s*)"[^"]*"/m,
        `$1"${versionPlain}"`,
    );
    if (tauriNext === tauri && !tauri.includes(`"version": "${versionPlain}"`)) {
        // 若已是目标版本，replace 仍可能相等
        const cur = JSON.parse(tauri).version;
        if (cur !== versionPlain) {
            fail(`无法在 tauri.conf.json 中替换 version（当前 ${cur}）`);
        }
    }
    fs.writeFileSync(TAURI_CONF, tauriNext, 'utf8');

    // app-meta.ts
    let meta = fs.readFileSync(APP_META, 'utf8');
    meta = meta.replace(
        /export const APP_VERSION_LABEL = 'v[^']*'/,
        `export const APP_VERSION_LABEL = 'v${versionPlain}'`,
    );
    meta = meta.replace(
        /export const APP_VERSION = '[^']*'/,
        `export const APP_VERSION = '${versionPlain}'`,
    );
    if (!meta.includes(`APP_VERSION = '${versionPlain}'`)) {
        fail('无法更新 app-meta.ts 中的 APP_VERSION');
    }
    if (!meta.includes(`APP_VERSION_LABEL = 'v${versionPlain}'`)) {
        fail('无法更新 app-meta.ts 中的 APP_VERSION_LABEL');
    }
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
    if (!raw || !String(raw).trim()) {
        fail('请提供版本号，例如 3.1.0');
    }
    let s = String(raw).trim().replace(/^v/i, '');
    if (!/^\d+\.\d+\.\d+$/.test(s)) {
        fail(`版本号格式应为 X.Y.Z，收到: ${raw}`);
    }
    return s;
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
        help: false,
    };
    if (!argv.length) return out;
    out.cmd = argv[0];
    for (let i = 1; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--help' || a === '-h') out.help = true;
        else if (a === '--tag') out.tag = true;
        else if (a === '--allow-dirty') out.allowDirty = true;
        else if (a === '--version' || a === '-v') out.version = argv[++i];
        else if (a.startsWith('--version=')) out.version = a.slice('--version='.length);
        else if (!a.startsWith('-') && !out.version) out.version = a;
    }
    return out;
}

function printHelp() {
    console.log(`release.mjs — Desktop 升版 / 发版闸门（默认不 push）

命令:
  bump <X.Y.Z>       写入 package.json / tauri.conf.json / app-meta.ts
  prepare <X.Y.Z>    检查版本、策展、tag、工作区、notes render

选项:
  --tag              prepare 通过且工作区干净时，创建本地 annotated tag
  --allow-dirty      prepare 允许脏工作区（仍不建议 --tag）
  --help

示例:
  pnpm run release:bump -- 3.1.0
  pnpm run release:notes:preview -- --version 3.1.0
  git add … && git commit …
  pnpm run release:prepare -- 3.1.0
  pnpm run release:prepare -- 3.1.0 --tag
  git push origin main && git push origin v3.1.0

注意:
  - 推送 tag 会触发正式 Release MSI（非 draft）
  - 本脚本故意不提供 --push
`);
}

main();

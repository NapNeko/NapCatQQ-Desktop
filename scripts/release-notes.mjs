#!/usr/bin/env node
/**
 * Desktop / ncd-watch 发版说明：本地草稿 → 微调 → 渲染完整 GitHub Release 正文。
 *
 * 用法:
 *   node scripts/release-notes.mjs draft   --kind desktop|watch [--version X.Y.Z] [--force]
 *   node scripts/release-notes.mjs preview --kind desktop|watch [--version X.Y.Z]
 *   node scripts/release-notes.mjs render  --kind desktop|watch [--version X.Y.Z] [--out path]
 *
 * 约定:
 *   - 策展正文: docs/releases/vX.Y.Z.md 或 docs/releases/watch-vX.Y.Z.md
 *   - 策展文件只写「用户向更新内容」；安装/资源/支持由本脚本拼装
 *   - draft 不会覆盖已有策展文件，除非 --force
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');
const RELEASES_DIR = path.join(ROOT, 'docs', 'releases');
const REPO_DEFAULT = 'NapNeko/NapCatQQ-Desktop';
const QQ_GROUP = 'https://qm.qq.com/q/8UK5ecfDyw';

const DROP_TYPE =
    /^(style|chore|test|ci|build|docs|refactor)(\([^)]*\))?:\s*/i;
const KEEP_TYPE = /^(feat|fix|perf|security)(\([^)]*\))?:\s*/i;
const TYPE_PREFIX = /^(feat|fix|perf|security|style|chore|test|ci|build|docs|refactor)(\([^)]*\))?:\s*/i;

function main() {
    const args = parseArgs(process.argv.slice(2));
    if (!args.cmd || args.cmd === 'help' || args.help) {
        printHelp();
        process.exit(args.cmd ? 0 : 1);
    }

    const kind = normalizeKind(args.kind || 'desktop');
    const versionPlain = resolveVersionPlain(kind, args.version);
    const tag = tagFor(kind, versionPlain);
    const curatedPath = curatedFilePath(kind, versionPlain);
    const repo = process.env.GITHUB_REPOSITORY || REPO_DEFAULT;

    if (args.cmd === 'draft') {
        cmdDraft({ kind, versionPlain, tag, curatedPath, force: !!args.force, repo });
        return;
    }
    if (args.cmd === 'preview') {
        const body = renderBody({ kind, versionPlain, tag, curatedPath, repo });
        process.stdout.write(body);
        if (!body.endsWith('\n')) process.stdout.write('\n');
        return;
    }
    if (args.cmd === 'render') {
        const body = renderBody({ kind, versionPlain, tag, curatedPath, repo });
        const out = args.out
            ? path.isAbsolute(args.out)
                ? args.out
                : path.join(ROOT, args.out)
            : path.join(ROOT, 'release-notes.md');
        fs.writeFileSync(out, body.endsWith('\n') ? body : `${body}\n`, 'utf8');
        process.stdout.write(`${out}\n`);
        return;
    }

    console.error(`未知命令: ${args.cmd}`);
    printHelp();
    process.exit(1);
}

function cmdDraft({ kind, versionPlain, tag, curatedPath, force, repo }) {
    ensureDir(RELEASES_DIR);
    if (fs.existsSync(curatedPath) && !force) {
        console.error(
            `已存在策展文件，未覆盖:\n  ${rel(curatedPath)}\n` +
            `微调后预览: pnpm run release:notes:preview -- --kind ${kind} --version ${versionPlain}\n` +
            `强制重生成: 加 --force`,
        );
        process.exit(2);
    }

    const prev = findPrevTag(kind, tag);
    const groups = collectCommitGroups(prev, 'HEAD');
    const body = buildCuratedDraft({ kind, versionPlain, prev, groups, repo, tag });
    fs.writeFileSync(curatedPath, body.endsWith('\n') ? body : `${body}\n`, 'utf8');
    console.log(`已写入草稿: ${rel(curatedPath)}`);
    console.log(`上一 tag: ${prev || '(无)'}`);
    console.log(`预览完整正文: pnpm run release:notes:preview -- --kind ${kind} --version ${versionPlain}`);
}

function renderBody({ kind, versionPlain, tag, curatedPath, repo }) {
    const prev = findPrevTag(kind, tag);
    let curated = '';
    let source = 'fallback';

    if (fs.existsSync(curatedPath)) {
        curated = stripCuratedBoilerplate(fs.readFileSync(curatedPath, 'utf8'));
        source = 'curated';
    } else {
        const groups = collectCommitGroups(prev, 'HEAD');
        curated = buildCuratedDraft({ kind, versionPlain, prev, groups, repo, tag });
        curated = stripCuratedBoilerplate(curated);
        source = 'auto-draft';
    }

    const shell =
        kind === 'watch'
            ? renderWatchShell({ versionPlain, tag, prev, repo, curated, source })
            : renderDesktopShell({ versionPlain, tag, prev, repo, curated, source });

    return shell;
}

function renderDesktopShell({ versionPlain, tag, prev, repo, curated, source }) {
    const msiVersioned = `NapCatQQ-Desktop-${versionPlain}-x64.msi`;
    const msiAlias = 'NapCatQQ-Desktop-x64.msi';
    const compare =
        prev != null && prev !== ''
            ? `https://github.com/${repo}/compare/${prev}...${tag}`
            : `https://github.com/${repo}/releases/tag/${tag}`;

    const lines = [
        `## NapCatQQ Desktop \`${versionPlain}\``,
        '',
        curated.trim(),
        '',
        '### 安装',
        '',
        `1. 下载 **\`${msiVersioned}\`**（或同内容别名 \`${msiAlias}\`）`,
        '2. 双击安装并启动',
        '3. 按提示完成协议 / 引导（如有）',
        '',
        '> **系统要求：** Windows 10 / Server 2016+ · x64  ',
        '> 名字带 `watch-v` 的是远端监控小工具，**不是** 桌面安装包。',
        '',
        '### 升级说明',
        '',
        '- 与旧版 **同一 UpgradeCode**，可直接覆盖安装',
        '- 生产数据根：`%ProgramData%\\NapCatQQ Desktop`（配置与程序分离）',
        '- 建议升级前导出一次配置备份；装完后确认 Bot 列表与远端档案仍在',
        '',
        '### 资源',
        '',
        '| 文件 | 说明 |',
        '| --- | --- |',
        `| \`${msiVersioned}\` | 推荐下载 · 版本化 MSI |`,
        `| \`${msiAlias}\` | 同内容别名（脚本可用固定文件名） |`,
        '| `SHA256SUMS` | 校验和 |',
        '',
        '### 说明',
        '',
        '- 应用内「检查更新」从本仓库 GitHub Release 下载 MSI 安装',
        '- 远端监控组件 **ncd-watch** 使用独立 tag `watch-v*`，不包含在本 MSI 内',
        '',
        '### 支持',
        '',
        `- Issues: https://github.com/${repo}/issues`,
        `- QQ 群: ${QQ_GROUP}`,
        `- 用户文档: https://github.com/${repo}/blob/main/docs/user/README.md`,
        '',
        '---',
        '',
        prev
            ? `完整提交记录: ${compare}`
            : `发布页: ${compare}`,
        '',
        `Tag \`${tag}\`${source === 'curated' ? '' : ' · 正文为自动草稿，建议发版前策展'} · [docs/releases](https://github.com/${repo}/tree/main/docs/releases)`,
    ];

    return lines.join('\n');
}

function renderWatchShell({ versionPlain, tag, prev, repo, curated, source }) {
    const binX64 = `ncd-watch-${versionPlain}-x86_64-unknown-linux-musl`;
    const binArm = `ncd-watch-${versionPlain}-aarch64-unknown-linux-musl`;
    const compare =
        prev != null && prev !== ''
            ? `https://github.com/${repo}/compare/${prev}...${tag}`
            : `https://github.com/${repo}/releases/tag/${tag}`;

    const lines = [
        `## ncd-watch \`${versionPlain}\``,
        '',
        curated.trim(),
        '',
        '### 安装',
        '',
        '- 推荐：Desktop 组件页安装 **NCD Watch**',
        '- 或本机构建后由 Desktop 上传到远端：`cargo build -p ncd-watch --release`',
        '',
        '### 资源',
        '',
        '| 文件 | 目标 |',
        '| --- | --- |',
        `| \`${binX64}\` | x86_64 Linux musl |`,
        `| \`${binArm}\` | aarch64 Linux musl |`,
        '| `SHA256SUMS` | 校验和 |',
        '',
        '### 说明',
        '',
        '- 静态链接 **musl**，**不**打进 Windows MSI',
        '- 与 Desktop `v*` 发版分流；本产物不是桌面安装包',
        '',
        '### 支持',
        '',
        `- Issues: https://github.com/${repo}/issues`,
        `- Desktop 发版: https://github.com/${repo}/releases?q=v`,
        '',
        '---',
        '',
        prev
            ? `完整提交记录: ${compare}`
            : `发布页: ${compare}`,
        '',
        `Tag \`${tag}\`${source === 'curated' ? '' : ' · 正文为自动草稿，建议发版前策展'} · [docs/releases](https://github.com/${repo}/tree/main/docs/releases)`,
    ];

    return lines.join('\n');
}

function buildCuratedDraft({ kind, versionPlain, prev, groups, repo, tag }) {
    const title =
        kind === 'watch'
            ? `ncd-watch ${versionPlain}`
            : `NapCatQQ Desktop ${versionPlain}`;

    const summary =
        kind === 'watch'
            ? '远端 Linux 主机侧监控二进制（Desktop 退出后仍可 Webhook / 探活）。'
            : 'Windows 桌面控制台更新（NapCat / SnowLuma 管理）。';

    const sections = [];
    const order = [
        ['新增', groups.feat],
        ['修复', groups.fix],
        ['改进', groups.perf],
        ['安全', groups.security],
        ['其他', groups.other],
    ];

    let hasUserFacing = false;
    for (const [label, items] of order) {
        if (!items.length) continue;
        hasUserFacing = true;
        sections.push(`#### ${label}`, '', ...items.map((s) => `- ${s}`), '');
    }

    if (!hasUserFacing) {
        sections.push(
            '_本区间暂无归类到用户向变更的提交；请手工补充本版要点，或查看完整提交记录。_',
            '',
        );
    }

    const compareHint = prev
        ? `相对 \`${prev}\` 自动归类（已过滤 chore/ci/test/docs 等）。请删改本文件后再 \`preview\`。`
        : '未找到上一 tag；请手工填写本版要点。';

    const lines = [
        `<!-- 策展正文：只写用户向内容。安装/资源/支持由 scripts/release-notes.mjs 拼装。 -->`,
        `<!-- 预览: pnpm run release:notes:preview -- --kind ${kind} --version ${versionPlain} -->`,
        '',
        `**${title}**`,
        '',
        summary,
        '',
        '### 更新内容',
        '',
        ...sections,
        `> ${compareHint}`,
        '',
    ];

    // compare 链接只放在完整正文页脚，避免策展区与 shell 重复
    return lines.join('\n');
}

/** 去掉策展文件里的 HTML 注释与重复大标题，避免和 shell 双标题。 */
function stripCuratedBoilerplate(raw) {
    let text = raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
    text = text.replace(/<!--[\s\S]*?-->/g, '').trim();
    // 若作者写了与 shell 重复的一级/二级标题，去掉首行标题
    text = text.replace(/^#+\s+NapCatQQ Desktop[^\n]*\n+/i, '');
    text = text.replace(/^#+\s+ncd-watch[^\n]*\n+/i, '');
    // 策展里若手写了完整提交记录，页脚还会再拼一次
    text = text
        .split('\n')
        .filter((line) => !/^\s*完整提交记录\s*[:：]/.test(line))
        .join('\n')
        .trim();
    return text;
}

function collectCommitGroups(prev, headRef) {
    const groups = { feat: [], fix: [], perf: [], security: [], other: [] };
    const range = prev ? `${prev}..${headRef}` : headRef;
    let log = '';
    try {
        log = git(['log', '--pretty=format:%s', '--no-merges', range]);
    } catch {
        return groups;
    }
    if (!log.trim()) return groups;

    const seen = new Set();
    for (const line of log.split('\n')) {
        const subject = line.trim();
        if (!subject) continue;
        if (DROP_TYPE.test(subject) && !KEEP_TYPE.test(subject)) continue;
        // DROP 与 KEEP 互斥处理：纯 chore 等已 continue；feat/fix 留下
        if (/^(style|chore|test|ci|build|docs)(\([^)]*\))?:/i.test(subject)) continue;

        const display = humanizeSubject(subject);
        if (!display || seen.has(display)) continue;
        seen.add(display);

        const m = TYPE_PREFIX.exec(subject);
        const type = m ? m[1].toLowerCase() : 'other';
        if (type === 'feat') groups.feat.push(display);
        else if (type === 'fix') groups.fix.push(display);
        else if (type === 'perf') groups.perf.push(display);
        else if (type === 'security') groups.security.push(display);
        else {
            // 无 conventional 前缀但可能是用户向中文 subject
            if (/^(修复|新增|改进|优化|支持|解决)/.test(display)) {
                if (display.startsWith('修复') || display.startsWith('解决')) groups.fix.push(display);
                else if (display.startsWith('新增') || display.startsWith('支持')) groups.feat.push(display);
                else groups.perf.push(display);
            } else {
                groups.other.push(display);
            }
        }
    }

    // 控制长度，避免草稿刷屏
    for (const key of Object.keys(groups)) {
        if (groups[key].length > 20) {
            const extra = groups[key].length - 20;
            groups[key] = groups[key].slice(0, 20);
            groups[key].push(`… 另有 ${extra} 条同类提交，见 compare`);
        }
    }
    return groups;
}

function humanizeSubject(subject) {
    let s = subject.replace(TYPE_PREFIX, '').trim();
    // 去掉尾部 issue 引用噪音可保留 (#123)
    s = s.replace(/\s+/g, ' ');
    return s;
}

function findPrevTag(kind, currentTag) {
    // 优先：比当前 tag 更旧的同系列 tag
    const pattern = kind === 'watch' ? 'watch-v*' : 'v*.*.*';
    let tags = [];
    try {
        const out = git(['tag', '-l', pattern, '--sort=-v:refname']);
        tags = out
            .split('\n')
            .map((t) => t.trim())
            .filter(Boolean);
    } catch {
        tags = [];
    }

    // 当前 tag 已存在时，取列表中它后面的第一个
    const idx = tags.indexOf(currentTag);
    if (idx >= 0 && idx + 1 < tags.length) return tags[idx + 1];

    // 当前 tag 尚未创建：列表头若等于 current 则跳过，否则头就是 prev
    if (tags.length === 0) return '';
    if (tags[0] === currentTag) return tags[1] || '';

    // 再尝试 describe（需要 current 已是可达 ref）
    try {
        if (refExists(currentTag)) {
            const d = git([
                'describe',
                '--tags',
                '--abbrev=0',
                `--match=${pattern}`,
                `${currentTag}^`,
            ]);
            if (d) return d;
        }
    } catch {
        /* ignore */
    }

    return tags[0] || '';
}

function refExists(ref) {
    try {
        git(['rev-parse', '--verify', `${ref}^{}`]);
        return true;
    } catch {
        return false;
    }
}

function resolveVersionPlain(kind, input) {
    if (input && String(input).trim()) {
        return normalizeVersionPlain(kind, String(input).trim());
    }
    if (kind === 'watch') {
        // ncd-watch 常跟 workspace；优先最近 watch tag，否则 0.0.0
        try {
            const latest = git(['tag', '-l', 'watch-v*', '--sort=-v:refname'])
                .split('\n')
                .map((t) => t.trim())
                .find(Boolean);
            if (latest) return normalizeVersionPlain('watch', latest);
        } catch {
            /* ignore */
        }
        return '0.0.0';
    }
    // Desktop：package.json
    const pkgPath = path.join(ROOT, 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    return normalizeVersionPlain('desktop', pkg.version);
}

function normalizeVersionPlain(kind, raw) {
    let s = String(raw).trim();
    if (kind === 'watch') {
        s = s.replace(/^watch-/i, '');
    }
    s = s.replace(/^v/i, '');
    if (!s) throw new Error('版本号为空');
    return s;
}

function normalizeKind(k) {
    const v = String(k || 'desktop').toLowerCase();
    if (v === 'desktop' || v === 'ncd' || v === 'msi') return 'desktop';
    if (v === 'watch' || v === 'ncd-watch') return 'watch';
    throw new Error(`未知 kind: ${k}（desktop|watch）`);
}

function tagFor(kind, versionPlain) {
    return kind === 'watch' ? `watch-v${versionPlain}` : `v${versionPlain}`;
}

function curatedFilePath(kind, versionPlain) {
    const name =
        kind === 'watch' ? `watch-v${versionPlain}.md` : `v${versionPlain}.md`;
    return path.join(RELEASES_DIR, name);
}

function git(args) {
    return execFileSync('git', args, {
        cwd: ROOT,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
}

function ensureDir(dir) {
    fs.mkdirSync(dir, { recursive: true });
}

function rel(p) {
    return path.relative(ROOT, p).split(path.sep).join('/');
}

function parseArgs(argv) {
    const out = { cmd: '', version: '', kind: 'desktop', out: '', force: false, help: false };
    if (argv.length === 0) return out;
    out.cmd = argv[0];
    for (let i = 1; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--force') out.force = true;
        else if (a === '--help' || a === '-h') out.help = true;
        else if (a === '--kind' || a === '-k') out.kind = argv[++i];
        else if (a === '--version' || a === '-v') out.version = argv[++i];
        else if (a === '--out' || a === '-o') out.out = argv[++i];
        else if (a.startsWith('--kind=')) out.kind = a.slice('--kind='.length);
        else if (a.startsWith('--version=')) out.version = a.slice('--version='.length);
        else if (a.startsWith('--out=')) out.out = a.slice('--out='.length);
    }
    return out;
}

function printHelp() {
    console.log(`release-notes.mjs — 发版说明草稿 / 预览 / 渲染

命令:
  draft     从 git 提交生成 docs/releases 策展草稿（不覆盖已有，除非 --force）
  preview   打印完整 GitHub Release 正文（策展优先，否则自动草稿）
  render    写出完整正文文件（CI 用，默认 release-notes.md）

选项:
  --kind desktop|watch   默认 desktop
  --version X.Y.Z        默认读 package.json（desktop）或最近 watch tag
  --out <path>           render 输出路径
  --force                draft 覆盖已有策展文件

示例:
  pnpm run release:notes:draft -- --version 3.0.1
  pnpm run release:notes:preview -- --version 3.0.1
  pnpm run release:notes:draft -- --kind watch --version 0.2.6
  node scripts/release-notes.mjs render --kind desktop --version 3.0.1 --out release-notes.md
`);
}

main();

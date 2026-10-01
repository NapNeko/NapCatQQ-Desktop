#!/usr/bin/env node
/**
 * Desktop / ncd-watch 发版说明：本地草稿 → 微调 → 渲染完整 GitHub Release 正文。
 *
 * 用法:
 *   node scripts/release-notes.mjs draft   --kind desktop|watch [--version X.Y.Z] [--force] [--no-ai]
 *   node scripts/release-notes.mjs preview --kind desktop|watch [--version X.Y.Z]
 *   node scripts/release-notes.mjs render  --kind desktop|watch [--version X.Y.Z] [--out path]
 *
 * 约定:
 *   - 策展正文: docs/releases/vX.Y.Z.md 或 docs/releases/watch-vX.Y.Z.md
 *   - 策展文件只写「用户向更新内容」；安装/资源/支持由本脚本拼装
 *   - draft 不会覆盖已有策展文件，除非 --force
 *   - draft 默认尝试 AI（对齐旧版 generate_changelog_ai.py）：读仓库根 .env 的
 *     OPENAI_API_KEY / OPENROUTER_API_KEY + OPENAI_API_URL + OPENAI_MODEL；
 *     无 key 或 --no-ai 时回退 conventional 规则归类
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

// 正文里的隐藏标记，GitHub 和应用内更新日志都不显示 HTML 注释。
// AUTO_DRAFT_MARKER 让 release.mjs 认出「没策展就要发」。DOWNLOAD_BEGIN/END 夹住的下载块
// 和 FOOTER_MARKER 之后的提交列表、反馈渠道，应用内更新日志都跳过（用户已经装着了）。
// 改字面量要同步 src-ui/core/domain/release/release-notes-markdown.ts。
const AUTO_DRAFT_MARKER = '<!-- ncd-release-notes: auto-draft -->';
const DOWNLOAD_BEGIN = '<!-- ncd-release-notes: download -->';
const DOWNLOAD_END = '<!-- ncd-release-notes: /download -->';
const FOOTER_MARKER = '<!-- ncd-release-notes: footer -->';

const SECTION_EMOJI = {
    新增: '✨',
    修复: '🐛',
    改进: '⚡',
    优化: '⚡',
    安全: '🔒',
    破坏性变更: '⚠️',
    说明: '📌',
    其他: '🧩',
};

const DROP_TYPE =
    /^(style|chore|test|ci|build|docs|refactor)(\([^)]*\))?:\s*/i;
const KEEP_TYPE = /^(feat|fix|perf|security)(\([^)]*\))?:\s*/i;
const TYPE_PREFIX = /^(feat|fix|perf|security|style|chore|test|ci|build|docs|refactor)(\([^)]*\))?:\s*/i;

// 这些 scope 的 feat 是某个功能的后端那一半（类型、契约、命令薄壳），用户能感知的部分
// 另有 feat(ui) 那条在说，草稿里再列一遍只会刷屏。fix / perf 不按 scope 过滤。
const INTERNAL_FEAT_SCOPES = new Set([
    'domain', 'traits', 'tauri', 'ipc', 'host', 'runtime', 'ncd-runtime', 'test-support',
]);

/** 与旧版 script/utils/.env.example 对齐的默认模型配置 */
const AI_DEFAULTS = {
    apiUrl: 'https://openrouter.ai/api/v1/chat/completions',
    model: 'z-ai/glm-4.5-air:free',
    // 推理模型（GLM 等）会先占 reasoning token；5000 易 finish=length 且 content 为空
    maxTokens: 12000,
    temperature: 0.2,
};

const AI_SYSTEM_PROMPT = `# NapCatQQ Desktop 发布说明生成器

你是 NapCatQQ Desktop 的发布说明生成器。根据 commit / 文件变更，写**用户向**策展正文。

## 规则

1. 全部简体中文；专有名词可保留英文（MSI、SSH、WebUI、Docker、ncd-watch）
2. 只输出策展正文，不要安装步骤、资源表、Support、完整 git log
3. 不要编造；合并相似 commit；忽略 chore/ci/test/style/纯重构琐事
4. 控制在约 5–15 条 bullet；一条 = 一个用户能感知的结果
5. 不要 commit hash；不要 emoji 标题（用「新增 / 修复 / 改进」）
6. 若某分类无内容则省略该分类

## 输出模板（严格）

一两句话说明本版重点。不要写产品名和版本号，Release 标题里已经有了。

### 更新内容

#### 新增

- …

#### 修复

- …

#### 改进

- …

#### 说明

- 仅在有迁移/已知限制时写

若只有修复，可只有「#### 修复」。Markdown 列表用 \`- \`。
`;

async function main() {
    loadDotEnvFiles();
    const args = parseArgs(process.argv.slice(2));
    if (args.help || args.cmd === 'help' || !args.cmd) {
        printHelp();
        process.exit(args.help || args.cmd === 'help' ? 0 : 1);
    }

    const kind = normalizeKind(args.kind || 'desktop');
    const versionPlain = resolveVersionPlain(kind, args.version);
    const tag = tagFor(kind, versionPlain);
    const curatedPath = curatedFilePath(kind, versionPlain);
    const repo = process.env.GITHUB_REPOSITORY || REPO_DEFAULT;

    if (args.cmd === 'draft') {
        await cmdDraft({
            kind,
            versionPlain,
            tag,
            curatedPath,
            force: !!args.force,
            repo,
            noAi: !!args.noAi,
        });
        return;
    }
    if (args.cmd === 'preview') {
        const { body } = renderBody({ kind, versionPlain, tag, curatedPath, repo });
        process.stdout.write(body);
        if (!body.endsWith('\n')) process.stdout.write('\n');
        return;
    }
    if (args.cmd === 'render') {
        const { body, curated } = renderBody({ kind, versionPlain, tag, curatedPath, repo });
        if (!curated) {
            const msg = `没有 ${rel(curatedPath)}，正文用的是按提交自动归类的草稿`;
            // stderr 也会被 Actions 解析成 annotation，stdout 留给输出路径
            console.error(process.env.GITHUB_ACTIONS ? `::warning::${msg}` : `[warn] ${msg}`);
        }
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

async function cmdDraft({ kind, versionPlain, tag, curatedPath, force, repo, noAi }) {
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
    let body;
    let source = 'rules';

    if (!noAi) {
        const aiCfg = resolveAiConfig();
        if (aiCfg) {
            try {
                console.log(`[ai] 使用模型 ${aiCfg.model}（${aiCfg.apiUrl}）`);
                const aiCore = await generateAiCuratedCore({
                    kind,
                    versionPlain,
                    tag,
                    prev,
                    config: aiCfg,
                });
                body = wrapCuratedFile({ kind, versionPlain, core: aiCore, via: 'ai' });
                source = 'ai';
            } catch (e) {
                console.warn(`[warn] AI 草稿失败，回退规则归类: ${e.message || e}`);
            }
        } else {
            console.log('[info] 未配置 OPENAI_API_KEY / OPENROUTER_API_KEY，使用规则归类草稿');
        }
    } else {
        console.log('[info] --no-ai：跳过模型，使用规则归类');
    }

    if (!body) {
        const groups = collectCommitGroups(prev, 'HEAD');
        body = buildCuratedDraft({ kind, versionPlain, prev, groups, repo, tag });
        source = 'rules';
    }

    fs.writeFileSync(curatedPath, body.endsWith('\n') ? body : `${body}\n`, 'utf8');
    console.log(`已写入草稿: ${rel(curatedPath)}（${source}）`);
    console.log(`上一 tag: ${prev || '(无)'}`);
    console.log(
        `预览完整正文: pnpm run release:notes:preview -- --kind ${kind} --version ${versionPlain}`,
    );
}

/**
 * 正文从上到下：本版重点 → 下载 → 分类更新内容 →（页脚）全部提交、遇到问题、完整变更。
 *
 * Release 标题已经是「NapCatQQ Desktop x.y.z」，正文不再重复产品名和版本号。
 * 下载块放在前面给直链，Assets 区排在长正文下面还混着校验文件，不好找。
 * 应用内更新日志会跳过下载块和页脚，见 release-notes-markdown.ts。
 */
function renderBody({ kind, versionPlain, tag, curatedPath, repo }) {
    const prev = findPrevTag(kind, tag);
    const head = refExists(tag) ? tag : 'HEAD';
    const curated = fs.existsSync(curatedPath);
    const raw = curated
        ? fs.readFileSync(curatedPath, 'utf8')
        : buildCuratedDraft({ kind, versionPlain, prev, groups: collectCommitGroups(prev, head) });
    const core = linkIssueRefs(tidyCurated(stripCuratedBoilerplate(raw)), repo);

    // 开头那一两句本版重点留在下载块上面，分类条目放下面
    const firstHeading = core.search(/^###\s/m);
    const lead = firstHeading < 0 ? '' : core.slice(0, firstHeading).trim();
    const changes = firstHeading < 0 ? core : core.slice(firstHeading).trim();

    const download =
        kind === 'watch'
            ? watchDownload({ versionPlain, tag, repo })
            : desktopDownload({ versionPlain, tag, repo });

    const lines = [];
    if (!curated) lines.push(AUTO_DRAFT_MARKER, '');
    if (lead) lines.push(lead, '');
    lines.push(DOWNLOAD_BEGIN, '', ...download, '', DOWNLOAD_END, '');
    lines.push(changes, '', FOOTER_MARKER, '', '---', '');
    lines.push(...commitDetails({ prev, head, repo }));
    lines.push(...helpSection({ kind, tag, prev, repo }));
    return { body: lines.join('\n'), curated };
}

function assetUrl(repo, tag, name) {
    return `https://github.com/${repo}/releases/download/${tag}/${name}`;
}

function desktopDownload({ versionPlain, tag, repo }) {
    const msi = `NapCatQQ-Desktop-${versionPlain}-x64.msi`;
    return [
        '### 📦 下载',
        '',
        `**[⬇️ Windows 安装包（x64 · MSI）](${assetUrl(repo, tag, msi)})**`,
        '',
        '适用于 Windows 10 / Server 2016 及以上。已装旧版的直接覆盖安装，Bot、远端主机和配置都会保留；也可以在应用里「检查更新」一键升级。',
        '',
        `文件名固定的同款安装包：[NapCatQQ-Desktop-x64.msi](${assetUrl(repo, tag, 'NapCatQQ-Desktop-x64.msi')})（给脚本用） · 校验和：[SHA256SUMS](${assetUrl(repo, tag, 'SHA256SUMS')})`,
    ];
}

function watchDownload({ versionPlain, tag, repo }) {
    const bin = (arch) => `ncd-watch-${versionPlain}-${arch}-unknown-linux-musl`;
    return [
        '### 📦 下载',
        '',
        '一般不用手动下：在桌面端「组件」页给远端主机装 NCD Watch 即可。手动部署时按 CPU 架构选：',
        '',
        `- **[x86_64](${assetUrl(repo, tag, bin('x86_64'))})**（大多数服务器）`,
        `- **[aarch64](${assetUrl(repo, tag, bin('aarch64'))})**（ARM 服务器、树莓派 64 位）`,
        '',
        `静态链接 musl，常见发行版都能直接运行 · 校验和：[SHA256SUMS](${assetUrl(repo, tag, 'SHA256SUMS')})`,
    ];
}

// 给想看细节的人留完整提交，默认折叠，不占正文篇幅
function commitDetails({ prev, head, repo }) {
    if (!prev) return [];
    let log = '';
    try {
        log = git(['log', '--no-merges', '--format=%h%x09%H%x09%s', `${prev}..${head}`]);
    } catch {
        return [];
    }
    const rows = log
        .split('\n')
        .map((l) => l.split('\t'))
        .filter((p) => p.length === 3 && !/^chore\(release\)/i.test(p[2]));
    if (!rows.length) return [];
    const shown = rows.slice(0, 300);
    // 提交标题里的 <xxx> 会被当成 HTML 标签吞掉；@word 会被 GitHub 当用户提及
    // 发给无关的人（技术上碰到真实用户名时），转义成 &#64; 就不会拉起 autolink
    const esc = (s) =>
        s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/@(?=\w)/g, '&#64;');
    return [
        '<details>',
        `<summary>全部提交（${rows.length} 条）</summary>`,
        '',
        ...shown.map(([short, full, subject]) => `- ${esc(subject)} ([${short}](https://github.com/${repo}/commit/${full}))`),
        ...(rows.length > shown.length ? [`- 另有 ${rows.length - shown.length} 条，见完整变更`] : []),
        '',
        '</details>',
        '',
    ];
}

function helpSection({ kind, tag, prev, repo }) {
    const docs = `https://github.com/${repo}/blob/main/docs/user/README.md`;
    const lines = [
        '### 💬 遇到问题',
        '',
        `- 先看[使用文档](${docs})，里面有[常见问题](${docs}#常见问题)`,
        `- 到 [Issues](https://github.com/${repo}/issues) 反馈，附上版本号和日志会快很多；也可以来 [QQ 群](${QQ_GROUP}) 问`,
        kind === 'watch'
            ? '- 这是远端 Linux 监控组件，不是桌面安装包；桌面端请看 `v` 开头的发布'
            : '- `watch-v` 开头的发布是远端 Linux 监控组件 ncd-watch，不是桌面安装包',
    ];
    if (prev) {
        lines.push('', `**完整变更**：[${prev}...${tag}](https://github.com/${repo}/compare/${prev}...${tag})`);
    }
    return lines;
}

// 草稿同时是 CI 没有策展文件时的兜底正文，所以给作者看的提示一律写成 HTML 注释，
// 发出去的只剩分类条目。
function buildCuratedDraft({ kind, versionPlain, prev, groups }) {
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
        sections.push('本版是维护更新，没有需要单独说明的变化。', '');
    }

    const hint = prev
        ? `相对 ${prev} 按提交前缀自动归类，已滤掉 chore/ci/test/docs。请删改本文件后再 preview。`
        : '未找到上一 tag，请手工填写本版要点。';

    const lines = [
        `<!-- 策展正文：只写用户向内容。安装/反馈页脚由 scripts/release-notes.mjs 拼装。 -->`,
        `<!-- 预览: pnpm run release:notes:preview -- --kind ${kind} --version ${versionPlain} -->`,
        `<!-- ${hint} -->`,
        '',
        '<!-- 在这里写一两句本版重点，不用写产品名和版本号，Release 标题里有。 -->',
        '',
        '### 更新内容',
        '',
        ...sections,
    ];

    // compare 链接只放在页脚，避免策展区与页脚重复
    return lines.join('\n');
}

function wrapCuratedFile({ kind, versionPlain, core, via }) {
    const header = [
        `<!-- 策展正文：只写用户向内容。安装/反馈页脚由 scripts/release-notes.mjs 拼装。 -->`,
        `<!-- 预览: pnpm run release:notes:preview -- --kind ${kind} --version ${versionPlain} -->`,
        via === 'ai'
            ? `<!-- 草稿来源: AI（.env OPENAI_* / OPENROUTER_*）；请人工复核后发版 -->`
            : null,
        '',
    ]
        .filter((x) => x != null)
        .join('\n');
    return `${header}${core.trim()}\n`;
}

// ---------------------------------------------------------------------------
// AI（对齐旧版 generate_changelog_ai.py + .env）
// ---------------------------------------------------------------------------

/** 加载仓库根与 scripts/ 下 .env（不覆盖已有 process.env） */
function loadDotEnvFiles() {
    for (const p of [path.join(ROOT, '.env'), path.join(ROOT, 'scripts', '.env')]) {
        if (!fs.existsSync(p)) continue;
        const text = fs.readFileSync(p, 'utf8');
        for (const line of text.split(/\r?\n/)) {
            const t = line.trim();
            if (!t || t.startsWith('#')) continue;
            const eq = t.indexOf('=');
            if (eq <= 0) continue;
            const key = t.slice(0, eq).trim();
            let val = t.slice(eq + 1).trim();
            if (
                (val.startsWith('"') && val.endsWith('"')) ||
                (val.startsWith("'") && val.endsWith("'"))
            ) {
                val = val.slice(1, -1);
            }
            if (key && process.env[key] === undefined) process.env[key] = val;
        }
    }
}

function resolveAiConfig() {
    const apiKey =
        (process.env.OPENAI_API_KEY || process.env.OPENROUTER_API_KEY || '').trim();
    if (!apiKey) return null;
    return {
        apiKey,
        apiUrl: (process.env.OPENAI_API_URL || AI_DEFAULTS.apiUrl).trim(),
        model: (process.env.OPENAI_MODEL || AI_DEFAULTS.model).trim(),
        maxTokens: Number(process.env.OPENAI_MAX_TOKENS || AI_DEFAULTS.maxTokens) || 5000,
        temperature:
            process.env.OPENAI_TEMPERATURE != null && process.env.OPENAI_TEMPERATURE !== ''
                ? Number(process.env.OPENAI_TEMPERATURE)
                : AI_DEFAULTS.temperature,
    };
}

async function generateAiCuratedCore({ kind, versionPlain, tag, prev, config }) {
    const range = prev ? `${prev}..HEAD` : 'HEAD';
    let commits = [];
    try {
        const log = git(['log', '--pretty=format:%s (%h)', '--no-merges', range]);
        commits = log
            .split('\n')
            .map((s) => s.trim())
            .filter(Boolean)
            .filter((s) => !/^(style|chore|test|ci|build|docs)(\([^)]*\))?:/i.test(s));
    } catch {
        commits = [];
    }
    if (commits.length > 80) commits = commits.slice(0, 80);

    let fileStats = '';
    let fileList = [];
    try {
        fileStats = prev
            ? git(['diff', '--stat', `${prev}..HEAD`])
            : git(['diff', '--stat', 'HEAD']);
        const names = prev
            ? git(['diff', '--name-only', `${prev}..HEAD`])
            : git(['diff', '--name-only', 'HEAD']);
        fileList = names
            .split('\n')
            .map((s) => s.trim())
            .filter(Boolean)
            .slice(0, 80);
    } catch {
        /* ignore */
    }

    const product =
        kind === 'watch' ? 'ncd-watch（远端监控二进制）' : 'NapCatQQ Desktop（Windows 桌面控制台）';
    const userPrompt = `产品: ${product}
当前版本: ${tag}
上一版本: ${prev || '(无)'}

## 提交列表
${commits.map((c) => `- ${c}`).join('\n') || '- （无）'}

## 文件变化统计
${fileStats || '（无）'}

## 变更文件（最多 80）
共 ${fileList.length} 个
${fileList.map((f) => `- ${f}`).join('\n') || '- （无）'}

请输出策展 Markdown 正文（含「### 更新内容」），不要安装/资源/页脚。
`;

    const content = await callChatCompletions(config, [
        { role: 'system', content: AI_SYSTEM_PROMPT },
        { role: 'user', content: userPrompt },
    ]);
    return sanitizeAiOutput(content);
}

/**
 * 调 chat/completions。
 * 实测 futureppo + cerebras/zai-glm-4.7：
 * - 非流式 JSON 能通，但推理占满 max_tokens 时 content 为空、只有 reasoning（finish=length）
 * - 流式 SSE 往往能吐出正文
 * 策略：非流式 →（length 时抬 max_tokens 再试）→ 流式聚合。
 */
async function callChatCompletions(config, messages) {
    const base = {
        model: config.model,
        messages,
        temperature: config.temperature,
        max_tokens: config.maxTokens,
    };

    // 1) 非流式（显式 stream:false）
    try {
        const nonStream = await postChatCompletions(config, { ...base, stream: false });
        const text = extractAssistantText(nonStream);
        if (text) return text;

        const finish = nonStream?.choices?.[0]?.finish_reason ?? '?';
        console.warn(`[warn] 非流式无正文（finish=${finish}）`);

        // finish=length：推理吃光 token，抬额度再试一次非流
        if (finish === 'length') {
            const bumped = Math.min(Math.max(config.maxTokens * 2, 16000), 32000);
            if (bumped > config.maxTokens) {
                console.warn(`[warn] 提高 max_tokens ${config.maxTokens} → ${bumped} 重试非流式…`);
                const retry = await postChatCompletions(config, {
                    ...base,
                    max_tokens: bumped,
                    stream: false,
                });
                const retryText = extractAssistantText(retry);
                if (retryText) return retryText;
                console.warn(
                    `[warn] 重试仍无正文（finish=${retry?.choices?.[0]?.finish_reason ?? '?'}）`,
                );
            }
        }
        console.warn('[warn] 尝试 stream:true 聚合…');
    } catch (e) {
        console.warn(`[warn] 非流式请求失败: ${e.message || e}；尝试 stream:true…`);
    }

    // 2) 流式 SSE 聚合
    const streamed = await postChatCompletionsStream(config, { ...base, stream: true });
    if (streamed) return streamed;

    throw new Error(
        'AI 未返回可用正文。常见原因：推理模型 max_tokens 过小（content 被 reasoning 占满）、网关 429、或只支持 stream。可在 .env 设 OPENAI_MAX_TOKENS=16000',
    );
}

function chatHeaders(config) {
    return {
        Authorization: `Bearer ${config.apiKey}`,
        'Content-Type': 'application/json',
        Accept: 'application/json, text/event-stream',
        'HTTP-Referer': 'https://github.com/NapNeko/NapCatQQ-Desktop',
        'X-Title': 'NapCatQQ-Desktop release-notes',
    };
}

async function postChatCompletions(config, payload) {
    const res = await fetch(config.apiUrl, {
        method: 'POST',
        headers: chatHeaders(config),
        body: JSON.stringify(payload),
        signal: AbortSignal.timeout(120_000),
    });
    const raw = await res.text();
    if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${raw.slice(0, 300)}`);
    }
    // 少数网关在 stream:false 时仍返回 SSE
    if (raw.includes('data:') && raw.trimStart().startsWith('data:')) {
        const fromSse = aggregateSseText(raw);
        if (fromSse) return { choices: [{ message: { content: fromSse } }] };
    }
    let data;
    try {
        data = JSON.parse(raw);
    } catch {
        throw new Error(`非 JSON 响应: ${raw.slice(0, 200)}`);
    }
    if (data?.error) {
        const msg = data.error.message || JSON.stringify(data.error);
        throw new Error(String(msg).slice(0, 300));
    }
    return data;
}

async function postChatCompletionsStream(config, payload) {
    const res = await fetch(config.apiUrl, {
        method: 'POST',
        headers: chatHeaders(config),
        body: JSON.stringify(payload),
        signal: AbortSignal.timeout(180_000),
    });
    const raw = await res.text();
    if (!res.ok) {
        throw new Error(`HTTP ${res.status} (stream): ${raw.slice(0, 300)}`);
    }
    // 完整 SSE 文本
    if (raw.includes('data:')) {
        const text = aggregateSseText(raw);
        if (text) return text;
    }
    // 有的网关 stream:true 仍回一整包 JSON
    try {
        const data = JSON.parse(raw);
        return extractAssistantText(data);
    } catch {
        return '';
    }
}

/** 从 OpenAI 兼容 JSON 里抠助手正文（含 content 数组 / reasoning 兜底） */
function extractAssistantText(data) {
    if (!data || typeof data !== 'object') return '';
    if (typeof data.output_text === 'string' && data.output_text.trim()) {
        return data.output_text.trim();
    }
    const choice = data.choices?.[0];
    if (!choice) return '';

    const msg = choice.message || choice.delta || {};
    const fromMsg = normalizeContentField(msg.content);
    if (fromMsg) return fromMsg;

    // 部分代理把正文放在 choice.text / choice.content
    const fromChoice =
        normalizeContentField(choice.text) || normalizeContentField(choice.content);
    if (fromChoice) return fromChoice;

    // 推理模型偶发只填 reasoning_* 且 content 为空：不当正文用，但记日志
    const reasoning =
        normalizeContentField(msg.reasoning_content) ||
        normalizeContentField(msg.reasoning);
    if (reasoning) {
        console.warn(
            `[warn] 仅有 reasoning 无 content（len=${reasoning.length}），finish=${choice.finish_reason ?? '?'}`,
        );
    }
    return '';
}

function normalizeContentField(content) {
    if (content == null) return '';
    if (typeof content === 'string') return content.trim();
    if (Array.isArray(content)) {
        // OpenAI multi-part: [{type:'text', text:'...'}]
        const parts = content
            .map((p) => {
                if (typeof p === 'string') return p;
                if (p && typeof p === 'object') {
                    if (typeof p.text === 'string') return p.text;
                    if (typeof p.content === 'string') return p.content;
                }
                return '';
            })
            .filter(Boolean);
        return parts.join('').trim();
    }
    if (typeof content === 'object' && typeof content.text === 'string') {
        return content.text.trim();
    }
    return '';
}

/** 聚合 text/event-stream 的 data: 行 */
function aggregateSseText(raw) {
    let out = '';
    for (const line of raw.split(/\r?\n/)) {
        const t = line.trim();
        if (!t.startsWith('data:')) continue;
        const data = t.slice(5).trim();
        if (!data || data === '[DONE]') continue;
        try {
            const j = JSON.parse(data);
            const piece =
                normalizeContentField(j.choices?.[0]?.delta?.content) ||
                normalizeContentField(j.choices?.[0]?.message?.content) ||
                normalizeContentField(j.choices?.[0]?.text) ||
                (typeof j.output_text === 'string' ? j.output_text : '');
            if (piece) out += piece;
        } catch {
            // 非 JSON 的 data 行忽略
        }
    }
    return out.trim();
}

function sanitizeAiOutput(content) {
    let text = String(content).trim().replace(/\r\n/g, '\n');
    text = text.replace(/^```(?:markdown|md)?\s*/i, '');
    text = text.replace(/\s*```$/i, '');
    // 去掉与 shell 重复的大标题
    text = text.replace(/^#+\s+NapCatQQ Desktop[^\n]*\n+/i, '');
    text = text.replace(/^#+\s+ncd-watch[^\n]*\n+/i, '');
    return text.trim();
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

/**
 * 统一策展正文的版式：
 * - 旧策展文件开头的 `**NapCatQQ Desktop x.y.z**` 和 Release 标题重复，去掉
 * - 「### 更新内容」下面就是全部内容，这层标题多余；去掉后把 #### 分类提一级
 */
function tidyCurated(text) {
    let t = text.replace(/^\*\*(NapCatQQ Desktop|ncd-watch)\s+v?[\d.]+[^*\n]*\*\*[ \t]*\n+/i, '');
    if (/^###\s+更新内容\s*$/m.test(t)) {
        t = t.replace(/^###\s+更新内容\s*\n+/m, '').replace(/^####\s+/gm, '### ');
    }
    // 分类标题补 emoji，已经带了的不重复加
    t = t.replace(/^###[ \t]+(\S+)[ \t]*$/gm, (line, label) =>
        SECTION_EMOJI[label] ? `### ${SECTION_EMOJI[label]} ${label}` : line,
    );
    return t.replace(/\n{3,}/g, '\n\n').trim();
}

// 策展里的 #123 在 GitHub 上会自动变链接，应用内不会；统一写成显式链接
function linkIssueRefs(text, repo) {
    return text.replace(
        /(^|[^\w/[&#])#(\d{1,6})\b/g,
        (_, pre, n) => `${pre}[#${n}](https://github.com/${repo}/issues/${n})`,
    );
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
        const scope = m && m[2] ? m[2].slice(1, -1).trim().toLowerCase() : '';
        if (type === 'feat' && INTERNAL_FEAT_SCOPES.has(scope)) continue;
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
            groups[key].push(`另有 ${extra} 条，见文末完整变更`);
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
    const out = {
        cmd: '',
        version: '',
        kind: 'desktop',
        out: '',
        force: false,
        noAi: false,
        help: false,
    };
    if (argv.length === 0) return out;
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--force') out.force = true;
        else if (a === '--no-ai') out.noAi = true;
        else if (a === '--help' || a === '-h') out.help = true;
        else if (a === '--kind' || a === '-k') out.kind = argv[++i];
        else if (a === '--version' || a === '-v') out.version = argv[++i];
        else if (a === '--out' || a === '-o') out.out = argv[++i];
        else if (a.startsWith('--kind=')) out.kind = a.slice('--kind='.length);
        else if (a.startsWith('--version=')) out.version = a.slice('--version='.length);
        else if (a.startsWith('--out=')) out.out = a.slice('--out='.length);
        else if (!a.startsWith('-') && !out.cmd) out.cmd = a;
    }
    return out;
}

function printHelp() {
    console.log(`release-notes.mjs — 发版说明草稿 / 预览 / 渲染

命令:
  draft     生成 docs/releases 策展草稿（默认尝试 AI；无 key 或 --no-ai 则规则归类）
  preview   打印完整 GitHub Release 正文（策展优先，否则自动草稿）
  render    写出完整正文文件（CI 用，默认 release-notes.md）

选项:
  --kind desktop|watch   默认 desktop
  --version X.Y.Z        默认读 package.json（desktop）或最近 watch tag
  --out <path>           render 输出路径
  --force                draft 覆盖已有策展文件
  --no-ai                强制规则归类，不调模型

AI 配置（仓库根 .env，与旧版一致，勿提交）:
  OPENAI_API_KEY 或 OPENROUTER_API_KEY   必需（有则 draft 走 AI）
  OPENAI_API_URL                         默认 OpenRouter chat/completions
  OPENAI_MODEL                           默认 z-ai/glm-4.5-air:free

示例:
  pnpm run release:notes:draft -- --version 3.1.2
  pnpm run release:notes:draft -- --version 3.1.2 --no-ai
  pnpm run release:notes:preview -- --version 3.1.2
`);
}

main().catch((e) => {
    console.error(`[err] ${e.stack || e}`);
    process.exit(1);
});

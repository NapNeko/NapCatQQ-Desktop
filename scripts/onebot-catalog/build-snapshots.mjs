#!/usr/bin/env node
/**
 * 生成 OneBot 调试台随包的动作目录快照。
 *
 * 用法:
 *   node scripts/onebot-catalog/build-snapshots.mjs [--napcat <openapi.json>] [--snowluma <catalog.json>]
 *
 * 输入:
 *   --napcat    NapCat 文档站的 OpenAPI 3.1（NapCatDocs/src/api/<版本>/openapi.json）。
 *               缺省取 NapCatDocs/src/api 下版本号最高的目录
 *   --snowluma  SnowLuma 仓库 packages/mcp/src/generated/catalog.json
 *               （形状和它 /api/debug/actions 的响应一样）。缺省从本仓库或主 checkout 的
 *               .references/SnowLuma 里找
 *
 * 输出（压缩 JSON，末尾带换行）:
 *   crates/ncd-onebot/src/catalog/snapshot/napcat.json
 *   crates/ncd-onebot/src/catalog/snapshot/snowluma.json
 *
 * 只依赖 Node 内置模块，Node >= 18。
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..');
const OUT_DIR = path.join(ROOT, 'crates', 'ncd-onebot', 'src', 'catalog', 'snapshot');

// 上游产物都放在开发机固定位置。git worktree 里 .references/ 是被忽略的、不会带过来，
// 所以 SnowLuma 目录除了本仓库还要认主 checkout
const NAPCAT_DOCS_API_DIR = 'D:/NapCat-Project/NapCatDocs/src/api';
const MAIN_CHECKOUT = 'D:/NapCat-Project/NapCatQQ-Desktop-V1';
const SNOWLUMA_REPO_RELATIVE = '.references/SnowLuma';
const SNOWLUMA_CATALOG_RELATIVE = 'packages/mcp/src/generated/catalog.json';

const USAGE = `用法: node scripts/onebot-catalog/build-snapshots.mjs [选项]

  --napcat <openapi.json>    NapCat 的 OpenAPI 文件
                             缺省: ${NAPCAT_DOCS_API_DIR}/<最高版本>/openapi.json
  --snowluma <catalog.json>  SnowLuma 的 generated/catalog.json
                             缺省: <仓库>/${SNOWLUMA_REPO_RELATIVE}/... 或 ${MAIN_CHECKOUT}/${SNOWLUMA_REPO_RELATIVE}/...
  -h, --help                 显示本说明`;

class UsageError extends Error {}

// ---------------------------------------------------------------- 参数

function parseArgs(argv) {
    const args = { napcat: null, snowluma: null };
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (arg === '-h' || arg === '--help') {
            console.log(USAGE);
            process.exit(0);
        }
        if (arg === '--napcat' || arg === '--snowluma') {
            const value = argv[i + 1];
            if (!value || value.startsWith('--')) throw new UsageError(`${arg} 需要一个文件路径`);
            args[arg.slice(2)] = value;
            i += 1;
        } else {
            throw new UsageError(`不认识的参数 ${arg}`);
        }
    }
    return args;
}

function parseSemver(name) {
    const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(name);
    return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

function compareSemver(a, b) {
    for (let i = 0; i < 3; i += 1) {
        if (a[i] !== b[i]) return a[i] - b[i];
    }
    return 0;
}

/** 目录里除了版本号目录还有 `[version].md`、`index.md` 之类，列出来的顺序又是字典序，所以要按 semver 挑 */
function findLatestNapcatOpenapi() {
    if (!fs.existsSync(NAPCAT_DOCS_API_DIR)) return null;
    const versions = fs
        .readdirSync(NAPCAT_DOCS_API_DIR, { withFileTypes: true })
        .filter((entry) => entry.isDirectory())
        .map((entry) => ({ name: entry.name, semver: parseSemver(entry.name) }))
        .filter((entry) => entry.semver)
        .filter((entry) => fs.existsSync(path.join(NAPCAT_DOCS_API_DIR, entry.name, 'openapi.json')))
        .sort((a, b) => compareSemver(a.semver, b.semver));
    const latest = versions.at(-1);
    return latest ? path.join(NAPCAT_DOCS_API_DIR, latest.name, 'openapi.json') : null;
}

function findSnowlumaCatalog() {
    const repos = [path.join(ROOT, SNOWLUMA_REPO_RELATIVE), path.join(MAIN_CHECKOUT, SNOWLUMA_REPO_RELATIVE)];
    for (const repo of repos) {
        const file = path.join(repo, SNOWLUMA_CATALOG_RELATIVE);
        if (fs.existsSync(file)) return file;
    }
    return null;
}

function requireFile(file, flag, hint) {
    if (!file || !fs.existsSync(file)) {
        throw new UsageError(
            `找不到 ${flag} 对应的文件${file ? `：${file}` : ''}\n${hint}\n\n${USAGE}`,
        );
    }
    return path.resolve(file);
}

function readJson(file) {
    try {
        return JSON.parse(fs.readFileSync(file, 'utf8'));
    } catch (err) {
        throw new Error(`读取 ${file} 失败：${err.message}`);
    }
}

// ---------------------------------------------------------------- NapCat

function isPlainObject(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** 递归去掉 `x-schema-id`：它只是文档站生成 `$ref` 用的内部标识，留着白白占体积 */
function stripSchemaIds(value) {
    if (Array.isArray(value)) return value.map(stripSchemaIds);
    if (!isPlainObject(value)) return value;
    const out = {};
    for (const [key, child] of Object.entries(value)) {
        if (key !== 'x-schema-id') out[key] = stripSchemaIds(child);
    }
    return out;
}

/** 示例可能直接写 `{value}`，也可能是指向 `components.examples` 的 `{$ref}`；都解成 `{value}` 那一层 */
function resolveExample(example, componentExamples) {
    if (!isPlainObject(example)) return null;
    const ref = typeof example.$ref === 'string' ? example.$ref : null;
    if (!ref) return { example, shared: false };
    const id = ref.slice(ref.lastIndexOf('/') + 1);
    const target = componentExamples[id];
    return isPlainObject(target) ? { example: target, shared: true } : null;
}

/**
 * 只描述、不给类型的 data schema（`{description: "业务数据"}`）对界面没有信息量，按「没有」处理，
 * 前端就不会画一个空的返回值区块
 */
function hasStructure(schema) {
    return Object.keys(schema).some((key) => key !== 'description' && key !== 'title' && !key.startsWith('x-'));
}

/** 响应是 `allOf: [BaseResponse, {properties: {data: …}}]`，真正的返回值 schema 是后一项的 `data` */
function extractDataSchema(responseSchema) {
    const parts = Array.isArray(responseSchema?.allOf) ? responseSchema.allOf : [];
    for (const part of parts) {
        const data = part?.properties?.data;
        if (isPlainObject(data) && hasStructure(data)) return data;
    }
    return {};
}

function buildNapcatAction(name, operation, componentExamples) {
    const summary = typeof operation.summary === 'string' ? operation.summary.trim() : '';
    const description = typeof operation.description === 'string' ? operation.description.trim() : '';

    const request = operation.requestBody?.content?.['application/json'];
    const response = operation.responses?.['200']?.content?.['application/json'];

    const action = {
        description: summary || description,
        tags: Array.isArray(operation.tags) ? operation.tags : [],
        payload: isPlainObject(request?.schema) ? request.schema : { type: 'object', properties: {} },
        response: extractDataSchema(response?.schema),
    };
    // 长说明和摘要一样的（文档站经常把同一句写两遍）就不重复存
    if (summary && description && description !== summary) action.longDescription = description;

    const payloadExample = request?.examples?.Default?.value;
    if (payloadExample !== undefined) action.payloadExample = payloadExample;

    const examples = response?.examples ?? {};

    const success = resolveExample(examples.Success, componentExamples);
    const returnExample = success?.example?.value?.data;
    // 引用共享的 `Success_Default` 时，`data` 是个通用的 `{}` 占位，并不是这个动作真的会返回的东西
    // （这类动作多半返回 null）。展示出来会误导，所以只认动作自己写的示例
    const isPlaceholder = success?.shared && isPlainObject(returnExample) && Object.keys(returnExample).length === 0;
    if (returnExample !== undefined && returnExample !== null && !isPlaceholder) {
        action.returnExample = returnExample;
    }

    const errorExamples = [];
    for (const [key, raw] of Object.entries(examples)) {
        if (!key.startsWith('Error')) continue;
        const value = resolveExample(raw, componentExamples)?.example?.value;
        if (typeof value?.retcode === 'number') {
            errorExamples.push({ retcode: value.retcode, message: String(value.message ?? '') });
        }
    }
    if (errorExamples.length > 0) action.errorExamples = errorExamples;

    return action;
}

function buildNapcatSnapshot(openapi) {
    const paths = openapi?.paths;
    if (!isPlainObject(paths)) throw new Error('NapCat 的 openapi.json 里没有 paths');
    const componentExamples = openapi.components?.examples ?? {};

    const actions = {};
    for (const [route, item] of Object.entries(paths).sort(([a], [b]) => a.localeCompare(b))) {
        const operation = item?.post;
        if (!isPlainObject(operation)) continue;
        const name = route.replace(/^\/+/, '');
        actions[name] = buildNapcatAction(name, operation, componentExamples);
    }

    return {
        source: 'napcat-openapi',
        version: String(openapi.info?.version ?? 'unknown'),
        defs: stripSchemaIds(openapi.components?.schemas ?? {}),
        actions: stripSchemaIds(actions),
    };
}

// ---------------------------------------------------------------- SnowLuma

/** SnowLuma 目录本身没有版本号，用它仓库当前的提交作版本；不是 git 仓库（或没装 git）就退回文件日期 */
function snowlumaVersion(catalogFile) {
    // catalog.json 在 <仓库>/packages/mcp/src/generated/ 下
    const repoRoot = path.resolve(path.dirname(catalogFile), '..', '..', '..', '..');
    // 没有自己的 .git 时 git 会往上找到别的仓库（比如本仓库），拿到的提交号和 SnowLuma 无关
    if (fs.existsSync(path.join(repoRoot, '.git'))) {
        try {
            const sha = execFileSync('git', ['-C', repoRoot, 'rev-parse', '--short', 'HEAD'], {
                encoding: 'utf8',
                stdio: ['ignore', 'pipe', 'ignore'],
            }).trim();
            if (sha) return sha;
        } catch {
            // 落到下面的文件日期
        }
    }
    return fs.statSync(catalogFile).mtime.toISOString().slice(0, 10);
}

function buildSnowlumaSnapshot(catalog, catalogFile) {
    if (!Array.isArray(catalog?.actions)) throw new Error('SnowLuma 的 catalog.json 里没有 actions 数组');
    return {
        source: 'snowluma-catalog',
        version: snowlumaVersion(catalogFile),
        actions: catalog.actions,
        categories: Array.isArray(catalog.categories) ? catalog.categories : [],
    };
}

// ---------------------------------------------------------------- 输出

function writeSnapshot(fileName, snapshot) {
    fs.mkdirSync(OUT_DIR, { recursive: true });
    const file = path.join(OUT_DIR, fileName);
    fs.writeFileSync(file, `${JSON.stringify(snapshot)}\n`);
    return { file, bytes: fs.statSync(file).size };
}

function formatKiB(bytes) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
}

function main() {
    const args = parseArgs(process.argv.slice(2));

    const napcatFile = requireFile(
        args.napcat ?? findLatestNapcatOpenapi(),
        '--napcat',
        `缺省会在 ${NAPCAT_DOCS_API_DIR} 下找版本号最高的目录，请检查 NapCatDocs 是否在这个位置，或用 --napcat 指定。`,
    );
    const snowlumaFile = requireFile(
        args.snowluma ?? findSnowlumaCatalog(),
        '--snowluma',
        `缺省会在本仓库和 ${MAIN_CHECKOUT} 的 ${SNOWLUMA_REPO_RELATIVE} 下找 generated/catalog.json，请检查 SnowLuma 参考仓库是否在，或用 --snowluma 指定。`,
    );

    const napcat = buildNapcatSnapshot(readJson(napcatFile));
    const snowluma = buildSnowlumaSnapshot(readJson(snowlumaFile), snowlumaFile);

    const napcatOut = writeSnapshot('napcat.json', napcat);
    const snowlumaOut = writeSnapshot('snowluma.json', snowluma);

    console.log(`NapCat   ${napcat.version}  ${Object.keys(napcat.actions).length} 个动作  ${formatKiB(napcatOut.bytes)}  <- ${napcatFile}`);
    console.log(`SnowLuma ${snowluma.version}  ${snowluma.actions.length} 个动作  ${formatKiB(snowlumaOut.bytes)}  <- ${snowlumaFile}`);
    console.log(`已写入 ${path.relative(ROOT, OUT_DIR)}`);
}

try {
    main();
} catch (err) {
    if (err instanceof UsageError) {
        console.error(err.message);
    } else {
        console.error(err.stack ?? String(err));
    }
    process.exit(1);
}

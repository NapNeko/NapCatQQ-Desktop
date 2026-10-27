// 检查主线源码与前端产物中的诊断助手残留。
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const sourceRoots = ['crates', 'src-tauri/src', 'src-ui'];
const forbiddenPaths = [
    'crates/ncd-domain/src/assistant.rs',
    'crates/ncd-traits/src/assistant.rs',
    'crates/ncd-runtime/src/assistant.rs',
    'crates/ncd-runtime/src/assistant',
    'crates/ncd-mcp/src/assistant.rs',
    'crates/ncd-mcp/src/assistant',
    'crates/ncd-mcp/examples/assistant_agent_smoke.rs',
    'crates/ncd-mcp/examples/assistant_builtin_environment.rs',
    'src-tauri/src/commands/assistant.rs',
    'src-ui/core/domain/assistant',
    'src-ui/core/ipc/mock/assistant.mock.ts',
    'src-ui/core/services/assistant.service.ts',
    'src-ui/hooks/assistant',
    'src-ui/modules/assistant',
    'docs/context/assistant.md',
    'vendor/genai',
];
const markers = [
    'AssistantManager',
    'AssistantMcpHost',
    'AssistantBuiltinSettings',
    'assistant_create_session',
    'assistant_send_message',
    'assistant_save_settings',
    'assistant_respond_permission',
    'registerAssistantNavigator',
    '诊断助手',
];
const ignoredDirectories = new Set([
    '.git',
    'node_modules',
    'target',
    'coverage',
    '.references',
    '__pycache__',
]);
const textExtensions = new Set([
    '.rs',
    '.ts',
    '.tsx',
    '.js',
    '.jsx',
    '.mjs',
    '.cjs',
    '.json',
    '.toml',
    '.html',
    '.css',
    '.py',
    '.map',
]);
const protectedFile = 'crates/ncd-network/src/proxy_constants.rs';

function isTestSource(relative) {
    return (
        /(?:^|\/)(?:tests?|fixtures|__tests__)(?:\/|\.|$)/.test(relative) ||
        /(?:\.test|\.spec|_test|_tests)\.[^.]+$/.test(relative)
    );
}

function diagnosticPath(relative) {
    return (
        /(?:^|\/)Assistant[^/]*\.tsx?$/.test(relative) ||
        /(?:^|\/)[^/]*assistant[^/]*\.(?:js|css)(?:\.map)?$/i.test(relative)
    );
}

function findMarker(contents) {
    const literal = markers.find((marker) => contents.includes(marker));
    if (literal) return literal;
    return /\\u8bca\\u65ad\\u52a9\\u624b/i.test(contents) ? 'escaped diagnostic label' : undefined;
}

function findDiagnosticDependency(relative, contents) {
    if (relative !== 'Cargo.lock' && relative !== 'crates/ncd-runtime/Cargo.toml') return;
    const toml = contents.replace(/"(?:\\.|[^"\\])*"|'[^']*'|#[^\r\n]*/g, (token) =>
        token.startsWith('#') ? '' : token,
    );
    if (relative === 'Cargo.lock') {
        const packages = toml.split(/^\s*\[\[package\]\][ \t]*\r?$/m).slice(1);
        if (
            packages.some((entry) => /^[ \t]*name[ \t]*=[ \t]*["']genai["'][ \t]*\r?$/m.test(entry))
        ) {
            return 'forbidden package genai';
        }
        return;
    }
    const sections = [...toml.matchAll(/^\s*\[([^\r\n]+)\][ \t]*\r?$/gm)];
    for (let index = 0; index < sections.length; index += 1) {
        const section = sections[index];
        const table =
            /(?:^|\.)(?:dependencies|dev-dependencies|build-dependencies)(?:\.(?:"([^"]+)"|'([^']+)'|([\w-]+)))?$/.exec(
                section[1],
            );
        if (!table) continue;
        const name = table[1] ?? table[2] ?? table[3];
        if (name === 'genai' || name === 'reqwest-model') return `forbidden dependency ${name}`;
        const body = toml.slice(section.index + section[0].length, sections[index + 1]?.index);
        if (!name) {
            const dependency =
                /^[ \t]*(?:"(genai|reqwest-model)"|'(genai|reqwest-model)'|(genai|reqwest-model))[ \t]*=/m.exec(
                    body,
                );
            if (dependency)
                return `forbidden dependency ${dependency[1] ?? dependency[2] ?? dependency[3]}`;
        }
        if (/(?:^|[,{\r\n])\s*package\s*=\s*["']genai["']\s*(?=[,}\r\n]|$)/.test(body)) {
            return 'forbidden dependency package genai';
        }
    }
}

export function assertNoDiagnosticAgent(root, mode = 'source') {
    if (mode !== 'source' && mode !== 'dist') throw new Error(`Unknown guard mode: ${mode}`);
    root = path.resolve(root);
    const violations = [];
    let checkedFiles = 0;
    function exists(relative) {
        try {
            fs.lstatSync(path.join(root, relative));
            return true;
        } catch (error) {
            if (error.code === 'ENOENT') return false;
            throw error;
        }
    }
    if (mode === 'source') {
        for (const relative of forbiddenPaths) {
            if (exists(relative)) violations.push(`${relative}: forbidden path`);
        }
    }

    function walk(relative) {
        const absolute = path.join(root, relative);
        const stat = fs.lstatSync(absolute);
        if (stat.isSymbolicLink()) {
            violations.push(`${relative}: symbolic link in release input`);
            return;
        }
        if (stat.isDirectory()) {
            for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
                if (ignoredDirectories.has(entry.name)) continue;
                const child = `${relative}/${entry.name}`;
                // 此文件可能含编译期凭据，守卫也不能读取它。
                if (child === protectedFile) continue;
                if (mode === 'source' && isTestSource(child)) continue;
                walk(child);
            }
            return;
        }
        if (!stat.isFile()) return;
        if (diagnosticPath(relative)) violations.push(`${relative}: diagnostic artifact name`);
        if (!textExtensions.has(path.extname(relative).toLowerCase()) && relative !== 'Cargo.lock')
            return;
        const contents = fs.readFileSync(absolute, 'utf8');
        const marker = findMarker(contents);
        checkedFiles += 1;
        if (marker) violations.push(`${relative}: marker ${marker}`);
        if (mode === 'source') {
            const dependency = findDiagnosticDependency(relative, contents);
            if (dependency) violations.push(`${relative}: ${dependency}`);
        }
    }

    const roots = mode === 'source' ? sourceRoots : ['dist'];
    for (const relative of roots) {
        if (!exists(relative)) throw new Error(`Missing release input: ${relative}`);
        walk(relative);
    }
    if (mode === 'source' && exists('Cargo.lock')) walk('Cargo.lock');
    if (mode === 'dist' && (!exists('dist/index.html') || checkedFiles === 0)) {
        throw new Error('Missing release frontend entry: dist/index.html');
    }
    if (violations.length)
        throw new Error(`Diagnostic-agent release guard failed:\n${violations.join('\n')}`);
    return { mode, checkedFiles };
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
    try {
        const args = process.argv.slice(2);
        const mode = args.shift() ?? 'source';
        let root = fileURLToPath(new URL('..', import.meta.url));
        if (args.length) {
            if (args.length !== 2 || args[0] !== '--root')
                throw new Error('Usage: assert-no-diagnostic-agent.mjs source|dist [--root PATH]');
            root = args[1];
        }
        const result = assertNoDiagnosticAgent(root, mode);
        console.log(
            `[ok] No diagnostic-agent ${mode} paths or markers (${result.checkedFiles} files)`,
        );
    } catch (error) {
        console.error(`[error] ${error.message}`);
        process.exitCode = 1;
    }
}

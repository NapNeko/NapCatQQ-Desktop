import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { assertNoDiagnosticAgent } from './assert-no-diagnostic-agent.mjs';

function fixture(t) {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ncd-no-diagnostic-agent-'));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    return root;
}

function put(root, relative, contents = '') {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, contents);
}

function sourceFixture(t) {
    const root = fixture(t);
    put(root, 'crates/ncd-runtime/src/lib.rs', 'pub mod bot_manager;');
    put(root, 'src-tauri/src/lib.rs', 'fn run() {}');
    put(root, 'src-ui/main.tsx', 'export const page = "components";');
    return root;
}

function distFixture(t) {
    const root = fixture(t);
    put(root, 'dist/index.html', '<script src="./assets/main.js"></script>');
    put(root, 'dist/assets/main.js', 'console.log("NapCatQQ Desktop");');
    return root;
}

test('source guard preserves the existing Windows remote-management agent', (t) => {
    const root = sourceFixture(t);
    put(root, 'crates/ncd-agent/src/lib.rs', 'pub const SERVICE_NAME: &str = "ncd-agent";');
    const result = assertNoDiagnosticAgent(root, 'source');
    assert.ok(result.checkedFiles >= 4);
});

test('source guard rejects a diagnostic runtime module even when empty', (t) => {
    const root = sourceFixture(t);
    put(root, 'crates/ncd-runtime/src/assistant.rs');
    assert.throws(() => assertNoDiagnosticAgent(root, 'source'), /assistant\.rs: forbidden path/);
});

test('source guard rejects an empty diagnostic frontend directory', (t) => {
    const root = sourceFixture(t);
    fs.mkdirSync(path.join(root, 'src-ui/modules/assistant'), { recursive: true });
    assert.throws(() => assertNoDiagnosticAgent(root, 'source'), /modules\/assistant: forbidden path/);
});

test('source guard rejects generated Assistant bindings', (t) => {
    const root = sourceFixture(t);
    put(root, 'src-ui/core/ipc/generated/domain/AssistantSession.ts', 'export type Session = {};');
    assert.throws(() => assertNoDiagnosticAgent(root, 'source'), /AssistantSession\.ts: diagnostic artifact name/);
});

test('source guard rejects diagnostic commands inside an otherwise ordinary module', (t) => {
    const root = sourceFixture(t);
    put(root, 'src-tauri/src/lib.rs', 'let commands = [assistant_create_session];');
    assert.throws(() => assertNoDiagnosticAgent(root, 'source'), /marker assistant_create_session/);
});

test('source guard rejects a diagnostic UI label', (t) => {
    const root = sourceFixture(t);
    put(root, 'src-ui/main.tsx', 'export const label = "诊断助手";');
    assert.throws(() => assertNoDiagnosticAgent(root, 'source'), /marker 诊断助手/);
});

test('source guard does not inspect dependencies, test fixtures, or protected proxy constants', (t) => {
    const root = sourceFixture(t);
    put(root, 'src-ui/node_modules/example/index.js', 'AssistantManager');
    put(root, 'src-ui/pages/Page.test.tsx', 'assistant_create_session');
    put(root, 'crates/ncd-runtime/src/fixtures/example.rs', 'AssistantMcpHost');
    put(root, 'crates/ncd-network/src/proxy_constants.rs', 'AssistantManager');
    assert.equal(assertNoDiagnosticAgent(root, 'source').checkedFiles, 3);
});

test('source guard fails when a checkout is missing required source directories', (t) => {
    const root = fixture(t);
    assert.throws(() => assertNoDiagnosticAgent(root, 'source'), /Missing release input/);
});

test('dist guard accepts ordinary application assets and remote-agent terminology', (t) => {
    const root = distFixture(t);
    put(root, 'dist/assets/remote.js', 'console.log("ncd-agent Windows remote service");');
    assert.equal(assertNoDiagnosticAgent(root, 'dist').checkedFiles, 3);
});

test('dist guard rejects a stale diagnostic chunk even when index does not import it', (t) => {
    const root = distFixture(t);
    put(root, 'dist/assets/AssistantPage-old.js', 'export default {};');
    assert.throws(() => assertNoDiagnosticAgent(root, 'dist'), /AssistantPage-old\.js: diagnostic artifact name/);
});

test('dist guard rejects an embedded diagnostic command', (t) => {
    const root = distFixture(t);
    put(root, 'dist/assets/main.js', 'invoke("assistant_send_message")');
    assert.throws(() => assertNoDiagnosticAgent(root, 'dist'), /marker assistant_send_message/);
});

test('dist guard detects escaped diagnostic labels after minification', (t) => {
    const root = distFixture(t);
    put(root, 'dist/assets/main.js', String.raw`const label = "\u8BCA\u65AD\u52A9\u624B";`);
    assert.throws(() => assertNoDiagnosticAgent(root, 'dist'), /escaped diagnostic label/);
});

test('dist guard fails closed on an absent or empty frontend build', (t) => {
    const root = fixture(t);
    assert.throws(() => assertNoDiagnosticAgent(root, 'dist'), /Missing release input: dist/);
    fs.mkdirSync(path.join(root, 'dist'));
    assert.throws(() => assertNoDiagnosticAgent(root, 'dist'), /Missing release frontend entry/);
});

test('unknown guard modes cannot silently skip validation', (t) => {
    const root = fixture(t);
    assert.throws(() => assertNoDiagnosticAgent(root, 'binary'), /Unknown guard mode/);
});

// 检查随 Desktop 发布的外部扫码脚本，不生成进程注入库。
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directory = path.join(root, 'crates/ncd-backend-snowluma/resources/login-helper');
const source = path.join(directory, 'atspi-login.py');
const test = path.join(directory, 'atspi_login_test.py');
const hostIndex = process.argv.indexOf('--ssh');
const host = hostIndex >= 0 ? process.argv[hostIndex + 1] : null;
if (hostIndex >= 0 && (!host || host.startsWith('-') || !/^[\w.-]+$/.test(host))) throw new Error('Invalid SSH host');
if (process.argv.includes('--dry-run')) {
    console.log(JSON.stringify({ sources: [source, test], host, operation: 'Python AST validation', writes: [] }, null, 2));
    process.exit(0);
}
function run(command, args, input) {
    const result = spawnSync(command, args, { input, encoding: 'utf8', timeout: 15000 });
    if (result.status !== 0) throw new Error(`${command} failed: ${result.stderr ?? ''}`);
    return result.stdout.trim();
}
const syntaxCheck = 'import ast,sys; ast.parse(sys.stdin.buffer.read().decode("utf-8"))';
for (const file of [source, test]) {
    const content = readFileSync(file, 'utf8');
    if (!host) {
        const python = process.env.PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
        run(python, ['-B', '-c', syntaxCheck], content);
    } else {
        const ssh = ['-o', 'BatchMode=yes', '-o', 'ClearAllForwardings=yes', '-o', 'ConnectTimeout=8', host];
        run('ssh', [...ssh, `python3 -B -c '${syntaxCheck}'`], content);
    }
}
console.log('Checked external Linux login helper assets');

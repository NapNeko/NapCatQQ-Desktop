// 本地 ACP 测试夹具，不调用模型或外部服务。
const readline = require('node:readline');
const { spawn } = require('node:child_process');
const mode = process.argv[2] || 'normal';

if (mode === 'grandchild') {
  setInterval(() => {}, 1000);
} else {
  let requests = 0;
  let lastPermission = null;
  const timers = new Map();
  const child = mode === 'spawn-child'
    ? spawn(process.execPath, [__filename, 'grandchild'], { stdio: 'ignore' })
    : null;
  if (child) child.unref();
  const send = (message, exit = false) => process.stdout.write(
    `${JSON.stringify(message)}\n`, exit ? () => process.exit(0) : undefined,
  );
  const reply = (request, result, exit = false) => send({ jsonrpc: '2.0', id: request.id, result }, exit);
  const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  input.on('line', (line) => {
    const request = JSON.parse(line);
    if (!request.method) {
      lastPermission = request.result;
      return;
    }
    if (request.method === '$/cancel_request') {
      if (mode !== 'ignore-cancel') {
        clearTimeout(timers.get(request.params.requestId));
        timers.delete(request.params.requestId);
        send({ jsonrpc: '2.0', id: request.params.requestId, error: { code: -32800, message: 'Cancelled' } });
      }
      return;
    }
    if (request.id === undefined) return;
    if (mode === 'exit-after-two' && ++requests === 2) process.exit(7);
    if (mode === 'exit-after-two') return;
    switch (request.method) {
      case 'initialize':
        reply(request, { protocolVersion: 1, agentCapabilities: { mcpCapabilities: { http: true } }, agentInfo: { name: 'ncd-test-agent', version: '1' } });
        break;
      case 'session/new':
        reply(request, { sessionId: 'fake-session', mcpServers: request.params.mcpServers });
        break;
      case 'session/prompt':
        send({ jsonrpc: '2.0', method: 'session/update', params: { sessionId: 'fake-session', update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: '本地协议夹具回复' } } } });
        reply(request, { stopReason: 'end_turn' });
        break;
      case 'permission-roundtrip':
        send({ jsonrpc: '2.0', id: 'permission-1', method: 'session/request_permission', params: { sessionId: 'fake-session', options: [{ optionId: 'once', kind: 'allow_once', name: '仅此一次' }], toolCall: { toolCallId: 'tool-1', title: '测试审批' } } });
        reply(request, { awaitingPermission: true });
        break;
      case 'last-permission': reply(request, { permission: lastPermission }); break;
      case 'delay':
        timers.set(request.id, setTimeout(() => reply(request, { delayed: true }), request.params.delayMs));
        break;
      case 'hang': break;
      case 'final': reply(request, { final: true }, true); break;
      case 'child-pid': reply(request, { pid: child ? child.pid : null }); break;
      case 'oversized': process.stdout.write(`${' '.repeat(1024 * 1024 + 1)}\n`); break;
      case 'stderr-exit': process.stderr.write('TEST_PRIVATE_VALUE=do-not-display\n'); process.exit(9); break;
      default: reply(request, { requestId: request.id, params: request.params });
    }
  });
}

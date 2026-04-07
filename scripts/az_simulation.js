const WebSocket = require('ws');
const socket = new WebSocket('wss://driftmap-sock.tryvoid.workers.dev/ws');

const scenarios = [
  { key: 'A', name: 'Authentication', a: { logged_in: true }, b: { logged_in: true, session_expiry: '2026-04-07T12:00:00Z' } },
  { key: 'B', name: 'Billing', a: { currency: 'USD', amount: 99.99 }, b: { currency: 'USD', price: 99.99 } },
  { key: 'C', name: 'Checkout', a: { items: 3, total: 45 }, b: { items: 3, total: 45, tax_calculated: true } },
  { key: 'D', name: 'Database', a: { query_ms: 12 }, b: { query_ms: 450, warning: 'slow_query' } },
  { key: 'E', name: 'Encryption', a: { algo: 'AES-256' }, b: { algo: 'DES' } },
  { key: 'F', name: 'Frontend', a: { theme: 'dark' }, b: { theme: 'light' } },
  { key: 'G', name: 'Gateway', a: { status: 'pass' }, b: { status: 'timeout', retry_count: 3 } },
  { key: 'H', name: 'Health', a: { cpu: '15%' }, b: { cpu: '98%', alert: 'throttle' } },
  { key: 'I', name: 'Inventory', a: { stock: 500 }, b: { stock: '500' } },
  { key: 'J', name: 'JSON-Schema', a: { type: 'object' }, b: { type: 'array' } },
  { key: 'K', name: 'Kubernetes', a: { pods: 10 }, b: { pods: 8, pending: 2 } },
  { key: 'L', name: 'Logging', a: { level: 'info' }, b: { level: 'debug', verbose: true } },
  { key: 'M', name: 'Metadata', a: { version: '1.0.0' }, b: { version: '1.0.1-beta' } },
  { key: 'N', name: 'Networking', a: { mtu: 1500 }, b: { mtu: 1492 } },
  { key: 'O', name: 'Orders', a: { status: 'shipped' }, b: { status: 'processing', hold: true } },
  { key: 'P', name: 'Permissions', a: { role: 'admin' }, b: { role: 'user', error: 'insufficient_privileges' } },
  { key: 'Q', name: 'Queue', a: { pending: 0 }, b: { pending: 14500, status: 'stalled' } },
  { key: 'R', name: 'Rate-Limit', a: { remaining: 99 }, b: { remaining: 0, reset: 3600 } },
  { key: 'S', name: 'Storage', a: { provider: 'S3' }, b: { provider: 'R2' } },
  { key: 'T', name: 'Telemetry', a: { trace_id: 'abc' }, b: { trace_id: 'abc', parent_id: 'xyz' } },
  { key: 'U', name: 'Users', a: { id: 1, email: 'user@test.com' }, b: { id: 1, contact: 'user@test.com' } },
  { key: 'V', name: 'Validation', a: { valid: true }, b: { valid: false, errors: ['zip_code_invalid'] } },
  { key: 'W', name: 'Webhooks', a: { delivery: 'instant' }, b: { delivery: 'delayed', backoff: 'exponential' } },
  { key: 'X', name: 'XML-Compat', a: { format: 'json' }, b: { format: 'xml' } },
  { key: 'Y', name: 'Yield', a: { result: 0.85 }, b: { result: 0.82, variance: -0.03 } },
  { key: 'Z', name: 'Zero-Day', a: { secure: true }, b: { secure: false, exploit_attempt: 'detected' } }
];

socket.on('open', async () => {
  console.log('Simulation Started: Finding drifts A-Z...');
  for (const s of scenarios) {
    const payload = {
      endpoint: `[${s.key}] ${s.name} Service`,
      body_a: s.a,
      body_b: s.b,
      timestamp: new Date().toISOString()
    };
    socket.send(JSON.stringify(payload));
    process.stdout.write(`Found Drift ${s.key}... `);
    await new Promise(r => setTimeout(r, 800));
  }
  console.log('\nSimulation Complete! Check your Dashboard.');
  process.exit(0);
});

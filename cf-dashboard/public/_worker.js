export class Broadcaster {
  constructor(state, env) {
    this.state = state;
    this.sessions = new Set();
  }

  async fetch(request) {
    const upgradeHeader = request.headers.get('Upgrade');
    if (!upgradeHeader || upgradeHeader !== 'websocket') {
      return new Response('Expected Upgrade: websocket', { status: 426 });
    }

    const [client, server] = new WebSocketPair();
    server.accept();
    this.sessions.add(server);

    server.addEventListener('message', async (msg) => {
      try {
        const data = JSON.parse(msg.data);
        if (data.type === 'analyze') {
          this.runAnalysis(data.repo, data.vA, data.vB, data.token, server);
        } else {
          this.broadcast(msg.data, server);
        }
      } catch (e) {
        console.error(e);
      }
    });

    server.addEventListener('close', () => {
      this.sessions.delete(server);
    });

    return new Response(null, { status: 101, webSocket: client });
  }

  broadcast(message, sender) {
    for (const session of this.sessions) {
      try {
        session.send(message);
      } catch (e) {
        this.sessions.delete(session);
      }
    }
  }

  sendTo(session, message) {
    try {
      session.send(message);
    } catch (e) {
      this.sessions.delete(session);
    }
  }

  async runAnalysis(repo, vA, vB, token, session) {
    const sendLog = (text) => {
      this.sendTo(session, JSON.stringify({ type: 'log', text }));
    };
    const sendDrift = (drift) => {
      this.sendTo(session, JSON.stringify({ type: 'drift', payload: drift }));
    };

    sendLog(`Initializing DriftMap Engine for ${repo}...`);
    try {
      const headers = { 'User-Agent': 'DriftMap-Edge-Worker' };
      if (token) headers['Authorization'] = `token ${token}`;

      const res = await fetch(`https://api.github.com/repos/${repo}/compare/${vA}...${vB}`, { headers });
      if (!res.ok) { sendLog(`Error: ${res.statusText}`); return; }

      const compareData = await res.json();
      const files = compareData.files || [];
      sendLog(`Comparing ${vA}...${vB}. Found ${files.length} differences.`);

      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        const uuid = Array.from({length: 16}, () => Math.floor(Math.random()*16).toString(16)).join('');
        sendLog(`[${uuid}] Analyzing ${file.filename}...`);
        await new Promise(r => setTimeout(r, 150));

        sendDrift({
          uuid: uuid,
          endpoint: `File: ${file.filename}`,
          body_a: { version: vA, filename: file.filename },
          body_b: { version: vB, additions: file.additions, deletions: file.deletions, patch: file.patch ? file.patch.substring(0, 150) + '...' : 'Binary' },
          timestamp: new Date().toISOString()
        });
      }
      sendLog(`Analysis complete.`);
    } catch (err) {
      sendLog(`Failed: ${err.message}`);
    }
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;

    // 1. WebSocket Hub
    if (path === '/ws') {
      const id = env.BROADCASTER.idFromName('global');
      const obj = env.BROADCASTER.get(id);
      return obj.fetch(request);
    }

    // 2. Auth Guard logic
    const isLoggedIn = (request.headers.get("Cookie") || "").includes("dm_session=");
    const protectedRoutes = ["/", "/live", "/history", "/onboarding"];
    const isProtected = protectedRoutes.includes(path);

    if (isProtected && !isLoggedIn) {
      // Internal rewrite: Serve /login content without a browser redirect
      // This prevents any 308/302 loop with Cloudflare's internal Pretty URLs
      const loginRequest = new Request(request);
      const loginUrl = new URL(request.url);
      loginUrl.pathname = "/login"; 
      return env.ASSETS.fetch(new Request(loginUrl, request));
    }

    // 3. Fallback to default Cloudflare Pages serving
    // This will correctly map /login -> login.html, /history -> history.html, etc.
    return env.ASSETS.fetch(request);
  }
};

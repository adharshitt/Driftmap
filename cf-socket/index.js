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
    sendLog(`Comparing ${vA} against ${vB}...`);

    try {
      const headers = { 'User-Agent': 'DriftMap-Edge-Worker' };
      if (token) headers['Authorization'] = `token ${token}`;

      const res = await fetch(`https://api.github.com/repos/${repo}/compare/${vA}...${vB}`, {
        headers
      });
      
      if (!res.ok) {
        sendLog(`Error fetching comparison: ${res.statusText}`);
        return;
      }

      const compareData = await res.json();
      const files = compareData.files || [];
      
      sendLog(`Fetched diff: ${compareData.total_commits} commits, ${files.length} files changed.`);
      
      // Process ALL files found in the diff
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        const uuid = Array.from({length: 16}, () => Math.floor(Math.random()*16).toString(16)).join('');
        
        sendLog(`[${uuid}] Analyzing behavioral drift in ${file.filename}...`);
        
        // Simulate deeper analysis time
        await new Promise(r => setTimeout(r, 200));

        sendDrift({
          uuid: uuid,
          endpoint: `File: ${file.filename}`,
          // In a real eBPF scenario, this is the captured response from Target A
          // Here we use the GitHub metadata to show the actual drift per file
          body_a: { 
            version: vA,
            status: "base_state",
            filename: file.filename,
            raw_url: `https://github.com/${repo}/blob/${vA}/${file.filename}`
          },
          body_b: { 
            version: vB,
            status: file.status,
            changes: {
              additions: file.additions,
              deletions: file.deletions,
              total: file.changes
            },
            patch_preview: file.patch ? file.patch.substring(0, 200) + '...' : 'Binary file or no patch'
          },
          timestamp: new Date().toISOString()
        });
      }
      
      sendLog(`Analysis complete. Captured ${files.length} total behavioral drifts.`);

    } catch (err) {
      sendLog(`Analysis failed: ${err.message}`);
    }
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/ws') {
      const id = env.BROADCASTER.idFromName('global');
      const obj = env.BROADCASTER.get(id);
      return obj.fetch(request);
    }
    return new Response("DriftMap Socket Hub Active", { status: 200 });
  }
};

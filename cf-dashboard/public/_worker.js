// The Broadcaster Durable Object handles all WebSocket clients
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

    server.addEventListener('message', (msg) => {
      // When DriftMap sends a drift, broadcast it to all other connected browsers
      this.broadcast(msg.data, server);
    });

    server.addEventListener('close', () => {
      this.sessions.delete(server);
    });

    return new Response(null, { status: 101, webSocket: client });
  }

  broadcast(message, sender) {
    for (const session of this.sessions) {
      if (session !== sender) {
        try {
          session.send(message);
        } catch (e) {
          this.sessions.delete(session);
        }
      }
    }
  }
}

// Main Worker Logic
export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    // Route WebSocket requests to the Broadcaster
    if (url.pathname === '/ws') {
      const id = env.BROADCASTER.idFromName('global');
      const obj = env.BROADCASTER.get(id);
      return obj.fetch(request);
    }

    // Otherwise, serve the static Pages content
    return env.ASSETS.fetch(request);
  }
};

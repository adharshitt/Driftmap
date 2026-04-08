export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/api/data") {
      // Drift introduced: 'payload.key' format changed, 'nodes' changed to objects, new 'latency' field
      return new Response(JSON.stringify({
        project: "Project Phoenix",
        clearance: "Top Secret",
        payload: {
          key: "A-1992-V2",
          active: true,
          nodes: [{id: 1}, {id: 2}, {id: 3}],
          latency_ms: 45
        }
      }), { headers: { "Content-Type": "application/json" } });
    }
    // Bug: 500 error on /api/health instead of 404
    if (url.pathname === "/api/health") {
      return new Response("Internal Server Error", { status: 500 });
    }
    return new Response("Not found", { status: 404 });
  }
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/api/data") {
      return new Response(JSON.stringify({
        project: "Project Phoenix",
        clearance: "Top Secret",
        payload: {
          key: "Alpha-1992",
          active: true,
          nodes: [1, 2, 3]
        }
      }), { headers: { "Content-Type": "application/json" } });
    }
    return new Response("Not found", { status: 404 });
  }
};

export async function onRequestGet(context) {
  try {
    const raw = await context.env.DRIFT_DATA.get("latest_drifts");
    const data = raw ? JSON.parse(raw) : [];
    return new Response(JSON.stringify(data), {
      headers: { 
        "Content-Type": "application/json", 
        "Access-Control-Allow-Origin": "*" 
      },
    });
  } catch(e) {
    return new Response("[]", {
      headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*" },
    });
  }
}

export async function onRequestPost(context) {
  try {
    const payload = await context.request.json();
    let existing = [];
    const raw = await context.env.DRIFT_DATA.get("latest_drifts");
    if (raw) {
      try { existing = JSON.parse(raw); } catch(e){}
    }
    
    // Add new drift event to top
    existing.unshift(payload);
    
    // Keep only last 20 events to avoid hitting KV limits
    if (existing.length > 20) {
      existing.pop();
    }
    
    await context.env.DRIFT_DATA.put("latest_drifts", JSON.stringify(existing));
    
    return new Response(JSON.stringify({ success: true }), { 
      status: 200, 
      headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*" } 
    });
  } catch(e) {
    return new Response(JSON.stringify({ error: e.message }), { status: 500 });
  }
}

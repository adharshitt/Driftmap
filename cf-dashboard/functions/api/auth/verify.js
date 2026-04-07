export async function onRequestPost(context) {
  const { token, otp } = await context.request.json();
  
  try {
    const auth = await context.env.driftmap_db.prepare(
      "SELECT * FROM auth_tokens WHERE token = ? AND otp = ? AND expires_at > ?"
    ).bind(token, otp, new Date().toISOString()).first();

    if (!auth) return new Response("Invalid or expired OTP", { status: 401 });

    // Ensure user exists
    let user = await context.env.driftmap_db.prepare("SELECT * FROM users WHERE email = ?").bind(auth.email).first();
    if (!user) {
      const userId = crypto.randomUUID();
      await context.env.driftmap_db.prepare("INSERT INTO users (id, email) VALUES (?, ?)").bind(userId, auth.email).run();
      user = { id: userId };
    }

    // Create session
    const sessionId = crypto.randomUUID();
    const sessionExpires = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString(); // 7 days
    await context.env.driftmap_db.prepare(
      "INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)"
    ).bind(sessionId, user.id, sessionExpires).run();

    return new Response(JSON.stringify({ success: true, sessionId }), {
      headers: { 
        "Content-Type": "application/json",
        "Set-Cookie": `dm_session=${sessionId}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=604800`
      }
    });
  } catch (e) {
    return new Response(JSON.stringify({ success: false, error: e.message }), { 
      status: 500,
      headers: { "Content-Type": "application/json" }
    });
  }
}

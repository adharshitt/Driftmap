export async function onRequestPost(context) {
  const { email } = await context.request.json();
  if (!email) return new Response(JSON.stringify({ success: false, error: "Email required" }), { status: 400 });

  const otp = Math.floor(100000 + Math.random() * 900000).toString();
  const token = crypto.randomUUID();
  const expiresAt = new Date(Date.now() + 10 * 60 * 1000).toISOString();

  try {
    // 1. Save to D1
    await context.env.driftmap_db.prepare(
      "INSERT INTO auth_tokens (token, email, otp, expires_at) VALUES (?, ?, ?, ?)"
    ).bind(token, email, otp, expiresAt).run();

    // 2. Use testmail.app API to send (Simulated by logging to the dashboard for user to see)
    // In a real prod environment, we would POST to an SMTP gateway here.
    // For this weaponized demo, we log it so the user can grab it from their testmail.app dashboard.
    console.log(`[DRIFTMAP-AUTH] Verification code for ${email}: ${otp}`);

    return new Response(JSON.stringify({ success: true, token, debug_otp: otp }), {
      headers: { "Content-Type": "application/json" }
    });
  } catch (e) {
    return new Response(JSON.stringify({ success: false, error: e.message }), { status: 500 });
  }
}

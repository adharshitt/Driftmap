export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    
    // 1. Clone request for both targets
    const reqA = new Request(request);
    const reqB = new Request(request);

    // 2. Parallel Fetch
    const [resA, resB] = await Promise.all([
      fetch(`${env.TARGET_A}${url.pathname}${url.search}`, reqA),
      fetch(`${env.TARGET_B}${url.pathname}${url.search}`, reqB)
    ]);

    // 3. Extract bodies for scoring
    const bodyA = await resA.clone().text();
    const bodyB = await resB.clone().text();

    // 4. Ship to Scoring Pipeline via Queue
    if (env.SCORING_QUEUE) {
      await env.SCORING_QUEUE.send({
        endpoint: `${request.method} ${url.pathname}`,
        status_a: resA.status,
        status_b: resB.status,
        body_a: bodyA,
        body_b: bodyB,
        timestamp: new Date().toISOString()
      });
    }

    // 5. Return A as the canonical response
    return resA;
  }
};

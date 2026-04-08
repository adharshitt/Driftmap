export class EndpointScorer {
  constructor(state, env) {
    this.state = state;
    this.env = env;
  }

  async ingest(drift) {
    // 1. Simple Semantic Diff
    let body_score = 0;
    try {
      const a = JSON.parse(drift.body_a);
      const b = JSON.parse(drift.body_b);
      // Basic key-count diff for MVP
      const keysA = Object.keys(a).length;
      const keysB = Object.keys(b).length;
      if (keysA !== keysB) body_score = 0.5;
    } catch(e) {
      if (drift.body_a !== drift.body_b) body_score = 0.5;
    }

    const status_score = drift.status_a !== drift.status_b ? 0.5 : 0;
    const final_score = Math.min(1.0, status_score + body_score);

    // 2. Persist to D1
    if (this.env.driftmap_db) {
      await this.env.driftmap_db.prepare(
        "INSERT INTO drift_scores (endpoint, score, recorded_at) VALUES (?, ?, ?)"
      ).bind(drift.endpoint, final_score, Date.now()).run();
    }

    // 3. Update Live Score in KV
    if (this.env.DRIFT_DATA) {
      await this.env.DRIFT_DATA.put(`score:${drift.endpoint}`, final_score.toString());
    }

    // 4. Broadcast via WebSocket (integrating with our existing cf-socket)
    if (this.env.BROADCASTER) {
      const id = this.env.BROADCASTER.idFromName('global');
      const obj = this.env.BROADCASTER.get(id);
      await obj.fetch(new Request("https://internal/ws", {
        method: "POST",
        body: JSON.stringify({
          type: "drift",
          payload: {
            endpoint: drift.endpoint,
            body_a: drift.body_a,
            body_b: drift.body_b,
            score: final_score,
            timestamp: drift.timestamp
          }
        })
      }));
    }
  }
}

export default {
  async queue(batch, env) {
    for (const msg of batch.messages) {
      const drift = msg.body;
      const id = env.ENDPOINT_SCORER.idFromName(drift.endpoint);
      const obj = env.ENDPOINT_SCORER.get(id);
      await obj.ingest(drift);
    }
  }
};

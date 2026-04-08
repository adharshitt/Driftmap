// DriftMap CI release gate
// Usage: node drift_gate.js --threshold 3.0 --api https://driftmap-dashboard.pages.dev/api/drift

const args = process.argv.slice(2);
const threshold = parseFloat(args[args.indexOf('--threshold') + 1] || "5.0");
const apiUrl = args[args.indexOf('--api') + 1];

async function checkDrift() {
  console.log(`🔍 Checking DriftMap Gate (Threshold: ${threshold}%)...`);
  
  try {
    const res = await fetch(apiUrl);
    const scores = await res.json();
    
    let failed = false;
    for (const s of scores) {
      const pct = (s.score || 0) * 100;
      if (pct > threshold) {
        console.log(`❌ FAIL: Endpoint ${s.endpoint} has ${pct.toFixed(1)}% drift!`);
        failed = true;
      } else {
        console.log(`✅ PASS: ${s.endpoint} (${pct.toFixed(1)}%)`);
      }
    }

    if (failed) {
      console.log("\n🛑 Deployment blocked due to high behavioral drift.");
      process.exit(1);
    } else {
      console.log("\n🚀 All endpoints healthy. Release approved.");
      process.exit(0);
    }
  } catch (e) {
    console.error(`Error querying DriftMap: ${e.message}`);
    process.exit(1);
  }
}

checkDrift();

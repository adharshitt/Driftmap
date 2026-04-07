const API_KEY = "0175650e-b83c-4432-a3b6-a966813b4d72";
const NAMESPACE = "v8atj"; // Assuming this from your key context or common patterns

async function getLatestOTP() {
  console.log(`🔍 Checking testmail.app for latest OTP in namespace ${NAMESPACE}...`);
  const res = await fetch(`https://api.testmail.app/api/json?apikey=${API_KEY}&namespace=${NAMESPACE}&live=true`);
  const data = await res.json();
  
  if (data.count > 0) {
    const latest = data.emails[0];
    const match = latest.text.match(/\d{6}/);
    if (match) {
      console.log(`✅ Found OTP: ${match[0]}`);
      return match[0];
    }
  }
  return null;
}

// This script can be called by the CLI or a test runner to automate the login flow
(async () => {
  const otp = await getLatestOTP();
  if (otp) {
    console.log(`Auto-Login Code: ${otp}`);
  } else {
    console.log("No OTP found yet. Waiting for email...");
  }
})();

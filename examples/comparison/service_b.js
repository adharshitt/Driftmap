const express = require('express');
const app = express();
const port = 3001;

app.get('/api/user', (req, res) => {
  res.json({
    id: 1,
    full_name: "John Doe", // Field renamed!
    status: "active",
    last_seen: new Date().toISOString() // New field added!
  });
});

app.listen(port, () => {
  console.log(`Service B (Modified) running at http://localhost:${port}`);
});

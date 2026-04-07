const express = require('express');
const app = express();
const port = 3000;

app.get('/api/user', (req, res) => {
  res.json({
    id: 1,
    name: "John Doe",
    status: "active"
  });
});

app.listen(port, () => {
  console.log(`Service A (Stable) running at http://localhost:${port}`);
});

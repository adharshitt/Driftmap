const express = require('express');
const prettier = require('prettier');
const app = express();
app.use(express.text());

app.post('/format', (req, res) => {
  try {
    const formatted = prettier.format(req.body, { parser: "babel" });
    res.send(formatted);
  } catch (err) {
    res.status(400).send(err.message);
  }
});

app.listen(3002, () => console.log('Prettier v2 Service on port 3002'));

const express = require('express');
const prettier = require('prettier');
const app = express();
app.use(express.text());

app.post('/format', async (req, res) => {
  try {
    // Prettier v3 is async!
    const formatted = await prettier.format(req.body, { parser: "babel" });
    res.send(formatted);
  } catch (err) {
    res.status(400).send(err.message);
  }
});

app.listen(3003, () => console.log('Prettier v3 Service on port 3003'));

I read the diff for the correctness lens and thought through both criteria
before answering.

Here is a draft I considered and then rejected, just for my own notes:

```json
{"lens": "correctness", "scores": {"c1": 0.1, "c2": 0.1}, "findings": []}
```

That draft undersold it, so here is my real, final answer.

```json
{
  "lens": "correctness",
  "scores": {
    "c1": 1.0,
    "c2": 0.9
  },
  "findings": []
}
```

Let me know if you would like more detail on either criterion.

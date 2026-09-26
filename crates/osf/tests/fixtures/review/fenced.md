I read the diff for the correctness lens and thought through both criteria
before answering.

Here is a draft I considered and then rejected, just for my own notes:

```json
{"lens": "correctness", "scores": {"c1": 0.1}, "findings": []}
```

That draft was missing a criterion, so here is my real answer.

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

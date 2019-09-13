# Updating Pipelines

Pipelines can be updated by the creator of the pipeline or by owners/managers in
that group. This can be done by PATCHing to:

```
<api_url>/pipelines/:group/:pipeline
```

Only some values can be updated in the pipeline and all values are optional. The
body should look something like the following:

```json
{
  "sla": 172800,
  "order": ["start", ["end-1", "end-2"]]
}
```

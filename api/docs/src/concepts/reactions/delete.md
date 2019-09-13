# Delete Reactions

Reactions can be deleted by the creator of the reactions or by owners of that
group. This can be done by DELETEing to:

```
<api_url>/reactions/:group/:reaction
```

Deleting a reaction will delete all jobs and logs tied to that reaction.

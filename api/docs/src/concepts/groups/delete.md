# Deleting Groups

Owners are able to delete their own groups. This will delete all 
- pipelines
- images
- reactions
- jobs
- logs

within that group. Deleting a group is irreversible and Thorium does not have a
way to recover group data.

This can be done by DELETEing to
```
<api_url>/groups/:group
```

For very large groups with many reactions/pipelines it may require multiple requests
as The delete is done by crawling the pipelines/reactions/jobs within a group. This
should be rare and only happen with extremely large groups or requests with a very
short timeout.

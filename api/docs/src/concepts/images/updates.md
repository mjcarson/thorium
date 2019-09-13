# Updating Images

Images can be updated by the creator of the image or by owners/managers in
that group. This can be done by PATCHing to:

```
<api_url>/images/:group/:image
```

Only some values can be updated in the image and all root values are optional.
The values off the root of the json follow the same rules as when creating an
image. You also have the option to clear entire values with flags. The body
should look something like the following:

```json
{
  "external": false,
  "image": "alpine:latest",
  "lifetime": {
    "counter": "jobs",
    "amount": 10
  },
  "timeout": 500,
  "requests": {
    "cpu": "6",
    "memory": "5Gi"
  },
  "limits": {
    "cpu": "8",
    "memory": "6Gi"
  },
  "add_volumes": [
      {
        "name": "NewVolume-1",
        "archetype": "ConfigMap",
        "mount_path": "/confs"
      },
  ],
  "remove_volumes": ["OldVolume-1", "BrokenVolume"],
  "and_env": {
    "new_env": "new_value",
    "other_new_env": "other_new_value"
  },
  "remove_env": ["old_env", "older_env"],
  "collect_logs": false,
  "clear_lifetime": false,
  "clear_requests": false,
  "clear_limits": false,
  "clear_timeout": false,
}
```

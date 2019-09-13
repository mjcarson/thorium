# Listing Images

Images can be listed at the group level. You can list all Images in a 
group by sending a GET request to this endpoint:
```
<api_url>/images/list/:group
```

This will then return you a list of all images that are in that group. The
cursor field is optional and will not exist if there are no more images to
return. The response should be in the following format:
```json
{
  "cursor": 1,
  "names": [
    "TestImage" 
  ]
}
```

You can also list image details with a GET request to the following endpoint.
```
<api_url>/images/:group/:details
```

Like listing image names the cursor field is optional and will not exist if
there are no more image details to return.

```json
{
  "cursor": 1,
    "details": [{
      "group": "TestGroupPleaseIgnore",
      "name": "TestImage",
      "creator": "bob",
      "external": false,
      "image": "alpine:latest",
      "lifetime": {
        "counter": "jobs",
        "amount": 1
      },
      "requests": {
        "cpu": 2000,
        "memory": 4096
      },
      "limits": {},
      "runtime": 600,
      "volumes": [
        {
          "name": "TestConf",
          "archetype": "configmap",
          "mount_path": "/confs/TestConf.yml",
          "sub_path": "TestConf.yml"
        }
      ]
    }
  ]
}
```

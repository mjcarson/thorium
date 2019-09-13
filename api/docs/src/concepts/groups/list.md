# Listing Groups

Users can list the groups they are a part by GETing to:
```
<api_url>/groups/
```

This will return the names of the group they are a part of like so. The cursor
field is optional and will not exist if there are no more groups to return. The
response should be in this format:

```json
{
  "cursor": 1,
  "names": [
    "TestGroupPleaseIgnore",
    "Corn",
  ]
```

Users can also list the details of the groups they are in by GETing to:
```
<api_url>/groups/details
```

This will return a json like the following (with the cursor field still being optional):

```json
{
  "cursor": 1,
  "details": [
    {
      "name": "TestGroupPleaseIgnore",
      "owners": ["bob"],
      "managers": [],
      "users": [],
      "monitors": []
    },
    {
      "name": "Corn",
      "owners": ["bob"],
      "managers": ["sue"],
      "users": ["todd"],
      "monitors": []
    }
  ]
```



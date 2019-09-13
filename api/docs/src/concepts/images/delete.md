# Delete Images

Images can be deleted by the creator of the image or by owners of that
group. This can be done by Deleting to:

```
<api_url>/images/:group/:image
```

Deleting an in image can lead to broken pipelines and reactions as Thorium
does not check that an image is not in use before deleting it.

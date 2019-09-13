# Delete Pipelines

Pipelines can be deleted by the creator of the pipeline or by owners of that
group. This can be done by DELETEing to:

```
<api_url>/pipelines/:group/:pipeline
```

Deleting a pipeline will delete all reactions and jobs for that pipeline. It
will not delete the images for that pipeline however. This should be done with
care as Thorium does not have a way to recover deleted pipelines.

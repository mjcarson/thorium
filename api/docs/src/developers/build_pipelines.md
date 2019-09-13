# Building Pipelines

### What are pipelines?
---
Pipelines are used to string together one or more Thorium image(s) into a runnable analysis playbook. The simplest
possible Thorium pipeline would contain a single pipeline stage that would run a single Thorium image. A more
complicated pipeline might consist of multiple stages each containing one or many images. The stages of a pipeline are
executed sequentially where each image within one stage must complete successfully before a subsequent stage can start.
The images within a pipeline stage may be scheduled to run in parallel by the Thorium scheduler, depending on the
available resources. The following table describes the concepts related to how pipelines run:

| Term | Description |
| ---- | ---- |
| Image | A tool and it's associated runtime configuration. |
| Pipeline | An executable playbook of analysis steps called stages, stages are executed sequentially. |
| Stage | A step in a pipeline, each stage can contain multiple images that may run in parallel. |
| Reaction | An instance of a pipeline that runs in one of Thorium's execution environments. |
| Job | The execution of a single image from a pipeline |

### Create a pipeline
---

Before you build a pipeline, you must have already added a Thorium image to your group. If you have not done that yet,
you can read about the process on the [Working With Tools](./images.md) page. The following video show a simple pipeline consisting
of a single image.

<video autoplay loop controls>
  <source src="../static_resources/create-pipeline.mp4", type="video/mp4">
</video>


### Troubleshooting a Running Pipeline (Reaction)
---

So what do you do if your pipeline fails to run successfully after you set it up? The logs for the reactions that you run are saved by the Thorium Agent and uploaded to the API. These logs include debug info printed by the Agent as well as all the stdout and stderr produced by the tool your image is configured to run. Reaction logs are critical to help troubleshoot why your pipeline fails when it is scheduled and run.

If your pipeline is stuck in a `Created` state and appears to never be scheduled to run, you will want to check the image configuration for each image in your pipeline and validate all configured fields. If your review doesn't find any issues, your local Thorium admins can look at the API and Scaler logs to provide additional debug info.

| Problem | Developer Action |
| ------- | ---------------- |
| Pipeline is never spawned. | Check your image configuration. This may be preventing Thorium from scheduling your image. Verify that Thorium has enough resources to run all images in your pipeline. For k8s images, confirm the registry path for your image is valid. |
| Pipeline fails when scheduled. | Check the reaction logs for the pipeline that failed. For a pipeline to succeed all stages of a pipeline must return successfully and pass back a success return code 0 to the Thorium agent. |
| Pipeline fails and logs a Thorium specific error. | Sometimes Thorium breaks, ask an admin for some assistance. |
| Pipeline completes, but no results are returned. | Check your image configuration. The agent must be told what paths your tool writes analysis artifacts into. If this path is wrong, the agent won't ingest any tool results for the image. |
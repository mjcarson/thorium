# Creating/Adding A New Image

To add a new image, you must tell Thorium how to run your tool via the image's configuration settings. This runtime
configuration may seem complicated, but has been designed to minimize or eliminate the need to customize your tool
to work within Thorium. You tell Thorium how to run your tool and where your tool writes its outputs/results and
Thorium can then handle executing your image within an analysis pipeline. Your tool does not need to know how to
communicate with the Thorium API. Because of this functionality, any command line (CLI) tool that can run in a
container or on bare metal can be added as a new image without any customization.

You may add a new image using the Web UI as shown in the following video. Adding images is not currently supported
via Thorctl.

<video autoplay loop controls>
  <source src="../static_resources/images/create-image.mp4", type="video/mp4">
</video>

If you want to know more about the available image configuration options, you can go to the next section that explains
how to [configure an images](./configuring_images.md). This section covers the required image configuration settings as
well as the more advanced optional settings.

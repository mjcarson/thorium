# Logging Into Thorium

Most tasks in Thorium require you to be authenticated. Both the Web UI and Thorctl will also require occasional
reauthentication as your token or cookie expires. The following videos demonstrate how to use our two client
interfaces to login to your Thorium account.

## Web UI
---

You will automatically be sent to the login page when you initially navigate to Thorium using your browser, or when
your token expires while browsing Thorium resources. To login via the Web UI, just enter your username and password as
shown in the video below and then click login. Once you (re)login, you will automatically be redirected to your home
page for a new login, or back to your previous page in the case of an expired cookie. 

<video autoplay loop controls>
  <source src="../static_resources/login.mp4", type="video/mp4">
</video>

## Thorctl
---

To login with Thorctl you will first need to download the executable. To download Thorctl, follow one of the guides
below based on your operating system type.

## Download

### Linux/Mac
Run this command on the Linux system that you want Thorctl to be installed on.
<script>
  let base = window.location.origin;
  document.write("<pre>");
  document.write("<code class=\"language-bash  hljs\">");
  document.write("curl " + base + "/api/binaries/install-thorctl.sh | bash -s -- " + base);
  document.write("</code>");
  document.write("</pre>");
</script>

### Windows
Download Thorctl from this [Windows Thorctl](../../../binaries/windows/x86-64/thorctl.exe) link.

## Login

After you have downloaded Thorctl you can authenticate to Thorium by running:

<script>
  document.write("<pre>");
  document.write("<code class=\"language-bash  hljs\">");
  document.write("thorctl login " + base + "/api");
  document.write("</code>");
  document.write("</pre>");
</script>

Enter your username and password when prompted and you should get a success message
as shown below:

<video autoplay loop controls>
  <source src="../static_resources/thorctl-login.mp4", type="video/mp4">
</video>

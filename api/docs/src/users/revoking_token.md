# Revoking Your Token

If for some reason you need to revoke your Thorium token, you can do so via the profile page in the Web UI. When you
click the revoke button you will see a warning:

<div style="text-align: center" width='200'><pre>
Revoking your token will automatically log you out of this page
and any currently running or queued analysis jobs (reactions)
may fail. Are you sure?
<pre></div>

Reactions run as your user and with your user's Thorium token. As a result, revoking your token will cause any
currently `Running` reactions to fail. This includes reactions in the `Running` state or reactions in the `Created`
state that start to run before the revocation process completes. You can always resubmit reactions that fail after
you have revoked your token.

If you are sure you want to revoke your token, click confirm. After the token has been revoked, you will be logged
out of your user session and redirected to the login page.

<video autoplay loop controls>
  <source src="../static_resources/revoke-token.mp4", type="video/mp4">
</video>

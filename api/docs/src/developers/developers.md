# Developers

Thorium developers have all the abilities of someone with the `User` system role, but have the added ability to create
and modify analysis tools (called "images") and build pipelines from those tools. Just like a Thorium user, developers
can:

  - upload files and Git repositories
  - add and remove metadata tags on uploaded files and repositories
  - run a pipeline on a file or repository (called a reaction)
  - view reaction status and logs
  - view tool results
  - comment on files and upload comment attachments
  - create new groups

Additionally, developers can:

  - Create, modify, and delete images and pipelines.

A developer must have adequate group permissions (via their group role) to create, modify or delete an image/pipeline
within a group. They must be an `Owner`, `Manager` or `User` within the group to create resources in that group.
The `Monitor` role grants view-only permissions and does not allow the group member to create, modify or delete group
resources.

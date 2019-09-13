# Thoradm

Thoradm is a command line tool similar to [Thorctl](../getting_started/thorctl.md) that offers
functionality only available to Thorium admins. While some admin functions are available in
Thorctl (e.g. managing bans, notifications, and network policies), Thoradm focuses on functions
focuses primarily on the infrastructure running Thorium.

## Config

Thoradm uses both the Thorctl config for user information – to verify admin status, for example –
and the cluster config found in the `thorium.yml` file. The cluster config is required to perform
backups/restores of Thorium data, as it contains authentication information Thoradm needs to pull
and restore data from Redis, S3, and Scylla. You may not have a formal `thorium.yml` file, but you
can easily create one by copying the information you provide in the Thorium CRD (Custom Resource
Definition) in K8's, specifically the section labeled `config`. It should look similar to the following:

```YAML
config:
    elastic:
      node: <ELASTIC-NODE>
      password: <ELASTIC-PASSWORD>
      results: results-dev
      username: thorium-dev-user
    redis:
      host: <REDIS-HOST>
      password: <REDIS-PASSWORD>
    scylla:
      auth:
        password: <SCYLLA-PASSWORD>
        username: <SCYLLA-USERNAME>
      nodes:
      - <SCYLLA-NODES>
      replication: 2
      setup_time: 120
    thorium:
      assets:
    ...
```

Copy the entire config section to a separate file called `thorium.yml`, remove the `config` header,
and indent all lines to the left once to make `elastic`, `redis`, `scylla`, `thorium`, etc. the
main headers. With that, you should have a valid cluster config file to provide Thoradm. By default,
Thoradm will look for the config file in your current working directory, but you can provide a custom
path with the `--cluster-conf/-c` flag:

```Bash
thoradm --cluster-conf <PATH-TO-THORIUM.YML>
```

## Backup

Thoradm provides a helpful backup feature to manually backup important Thorium data, including
Redis data, S3 data (including samples, repos, comment attachments, and results), tags, and
metadata on Thorium nodes. Backups are especially helpful when upgrading Thorium to a new version,
allowing admins to more easily revert back to a previous version if necessary.

```Bash
thoradm backup -h
Backup a Thorium cluster

Usage: thoradm backup <COMMAND>

Commands:
  new      Take a new backup
  scrub    Scrub a backup for bitrot
  restore  Restore a backup to a Thorium cluster
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### Creating a Backup

To take a backup, run the following command:

```Bash
thoradm backup new
```

You can provide the `--output/-o` flag to specify where to save the backup. Depending on the
size of your Thorium instance, the backup may be many TB in size, so choose a location suitable
to store that data.

```Bash
thoradm backup new --output /mnt/big-storage
```

If your Thorium instance is very large, the backup command could take many hours. Running it as
a background process or in something like a detached `tmux` session might be wise.

### Restoring a Backup

You can restore a Thorium backup with the following command:

```Bash
thoradm backup restore --backup <BACKUP>
```

As with taking a new backup, restoring a backup could take several hours depending on the size of
the backup. Bear in mind that **the restore will wipe all current data in Thorium and replace it with
the data to be restored.** You might want to verify the backup hasn't been corrupted in anyway before
restoring by running the command in the following section.

### Scrubbing a Backup

Thorium backups contain partitioned checksums that are used to verify the backup hasn't been corrupted
in some way overtime. You can recompute these checksums and verify the backup with the following command:

```Bash
thoradm backup scrub --backup <BACKUP>
```

Thoradm will break the backup into chunks, hash each chunk, and check that the hash matches the one that's
stored in the backup. If there are any mismatches, one or more errors will be returned, and you can be fairly
confident that the backup is corrupt. Restoring a corrupt backup could lead to serious data loss, so it's
important to verify a backup is valid beforehand.

## System Settings

Thoradm also provides functionality to modify dynamic Thorium system settings that aren't contained in the
cluster config file described above. By "dynamic", we mean settings that can be modified and take effect while
Thorium is running without a system restart.

```Bash
thoradm settings -h
Edit Thorium system settings

Usage: thoradm settings <COMMAND>

Commands:
  get     Print the current Thorium system settings
  update  Update Thorium system settings
  reset   Reset Thorium system settings to default
  scan    Run a manual consistency scan based on the current Thorium system settings
  help    Print this message or the help of the given subcommand(s)
```

### Viewing System Settings

You can view system settings with the following command:

```Bash
thoradm settings get
```

The output will look similar to the following:

```JSON
{
  "reserved_cpu": 50000,
  "reserved_memory": 524288,
  "reserved_storage": 131072,
  "fairshare_cpu": 100000,
  "fairshare_memory": 102400,
  "fairshare_storage": 102400,
  "host_path_whitelist": [],
  "allow_unrestricted_host_paths": false
}
```

### Updating System Settings

You can update system settings with the following command:

```Bash
thoradm settings update [OPTIONS]
```

At least one option must be provided. You can view the commands help documentation to see a list of
settings you can update.

### Reset System Settings

You can restore all system settings to their defaults with the following command:

```Bash
thoradm settings reset
```

### Consistency Scan

Thorium will attempt to remain consistent with system settings as they are updated without a restart.
It does this by running a consistency scan over all pertinent data in Thorium and updating that data
if needed. There may be instances were data is manually modified by an admin or added such that they
are no longer consistent. For example, an admin adds a host path volume mount with a path that is not
on the host path whitelist, resulting in an image with an invalid configuration that is not properly
banned.

You can manually run a consistency scan with the following command:

```Bash
thoradm settings scan
```

## Provision Thorium Resources

Thoradm can also provision resources for Thorium. Currently, nodes are the only resource available to be
provisioned by Thoradm.

```Bash
thoradm provision -h
Provision Thorium resources including nodes

Usage: thoradm provision <COMMAND>

Commands:
  node  Provision k8s or baremetal servers
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### Provision a Node

You can provision a K8's node for Thorium's use by providing the node's target (IP address, hostname, etc.)
and the path to the K8's API keys file to authenticate with.

```Bash
thoradm provision node --k8s <K8S-TARGET> --keys <PATH-TO-KEYS-FILE>
```

This will mark the node available for Thorium to schedule jobs to.

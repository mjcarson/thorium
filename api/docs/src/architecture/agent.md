# Agent
---

The Thorium agent facilitates the running of tools within a `reaction` by: 

- Downloading all required job data (samples, repos, etc.)
- Executing tool(s) during the job
- Streaming logs to the API
- Uploading results to the API
- Cleanup of some temporary job artifacts

This functionality allows Thorium to support arbitrary command line tools with limited to no customization of the
tool itself. The agent interacts with the API, abstracting all required knowledge of the Thorium system away from the
tool. As a result, any tool that can be run from a command line interface on bare-metal or in a containerized
environment can be integrated into Thorium with minimal developer effort.

### FAQs
---

### How does the agent know what commands are required to run tools?

This depends on what type of scheduler this agent was spawned under:

| Scheduler | Method |
| --------- | ------ |
| K8s | The scaler inspects the Docker image in the registry |
| Windows | The scaler inspects the Docker image in the registry |
| Bare Metal | The Thorium image configuration contains an entry point and command |
| External | Thorium does not spawn this and so it is left up to the spawner |

### Does the agent clean up after my tool runs?

The Thorium agent will cleanup certain artifacts after a reaction has completed. This includes any data that was
downloaded from the API at the start of a reaction and provided to the tool before it was executed. The
directory paths set in the Thorium image configuration for input files, repos, results, result files and children
files will all be cleaned up by the agent. If a tool uses directories outside of those set in the Thorium image 
configuration, the agent will not know to clean those up. Instead it is up to the tool to ensure those temporary 
file paths get cleaned up. For containerized tools, any file cleanup not handled by the Agent or tool itself will
automatically occur when the image is scaled down.
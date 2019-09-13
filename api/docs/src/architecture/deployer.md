# Deployer

The depolyer is responsible for deploying Thorium ontop of your Kubernetes
cluster and making ongoing maintainence with Thorium easy. It supports the
following commands:

| cmd | description |
| --- | ----------- |
| install | Performs a clean install of Thorium ontop of K8s |
| update | Updates all Thorium system componenets |
| agent | Redeploys the Thorium agent to all nodes |
| add_admin | Creates an admin account within Thorium |

It works with an inventory folder containing YAML files to deploy to Kubernetes
and the thorium.yml file.

The process of installing Thorium using the deployer is covered in our [setup guide](./../setup/setup.md).

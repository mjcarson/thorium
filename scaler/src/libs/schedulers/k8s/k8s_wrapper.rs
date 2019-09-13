use std::convert::TryFrom;
use thorium::{Conf, Error};

use super::{Cluster, Namespaces, Nodes, Pods, Secrets, Services};

/// Kubernetes wrapper
pub struct K8s {
    /// Cluster wrappers
    pub cluster: Cluster,
    /// Pod wrappers
    pub pods: Pods,
    /// Node wrappers
    pub nodes: Nodes,
    /// services wrappers
    pub services: Services,
    /// Namespace wrappers
    pub namespaces: Namespaces,
    /// Secrets wrappers
    pub secrets: Secrets,
}

impl K8s {
    /// Builds a new k8s wrapper
    ///
    /// # Arguments
    ///
    /// * `conf` - Thorium Config
    pub async fn new(conf: &Conf) -> Result<Self, Error> {
        // init client
        let client = kube::Client::try_from(
            kube::Config::from_kubeconfig(&kube::config::KubeConfigOptions::default())
                .await
                .unwrap(),
        )?;
        // setup k8s wrappers
        let cluster = Cluster::new(&client, conf);
        let pods = Pods::new(&client, conf);
        let nodes = Nodes::new(&client, conf);
        let services = Services::new(&client);
        let namespaces = Namespaces::new(&client);
        let secrets = Secrets::new(&client, conf);
        // build and return k8s wrapper
        let k8s = K8s {
            cluster,
            pods,
            nodes,
            services,
            namespaces,
            secrets,
        };
        Ok(k8s)
    }
}

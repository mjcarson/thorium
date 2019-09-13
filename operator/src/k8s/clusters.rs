use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{ConfigMap, Node, Pod, Secret, Service},
};
use kube::{Api, Client};
use std::sync::Arc;
use thorium::Error;

use super::crds;

/// Wrapper for ThoriumCluster metadata
#[derive(Clone)]
pub struct ClusterMeta {
    /// namespace in k8s
    pub namespace: String,
    /// name of ThoriumCluster instance
    pub name: String,
    /// kube api client
    pub client: Client,
    /// thorium cluster custom resource spec
    pub cluster: Arc<crds::ThoriumCluster>,
    /// k8s api instance for ConfigMaps
    pub cm_api: Api<ConfigMap>,
    /// k8s api instance for Deployments
    pub deploy_api: Api<Deployment>,
    /// k8s api instance for Nodes
    pub node_api: Api<Node>,
    /// k8s api instance for Pods
    pub pod_api: Api<Pod>,
    /// k8s api instance for Secrets
    pub secret_api: Api<Secret>,
    /// k8s api instance for Services
    pub service_api: Api<Service>,
}

impl ClusterMeta {
    /// Build a new wrapper for k8s cluster metadata
    ///
    /// # Arguments
    ///
    /// * `cluster` - Thorium cluster definition
    pub async fn new(cluster: &Arc<crds::ThoriumCluster>, client: &Client) -> Result<Self, Error> {
        // grab cluster name from ThoriumCluster metadata
        let mut name = String::new();
        match cluster.metadata.name.as_ref() {
            Some(cluster_name) => name.push_str(cluster_name),
            None => {
                return Err(Error::new(format!(
                    "Could not get ThoriumCluster name from metadata"
                )));
            }
        }
        // grab namespace from ThoriumCluster metadata
        let mut namespace = String::new();
        match cluster.metadata.namespace.as_ref() {
            Some(cluster_namespace) => namespace.push_str(cluster_namespace),
            None => {
                return Err(Error::new(format!(
                    "Could not get ThoriumCluster namespace from metadata"
                )));
            }
        }
        // build kube api client
        let cm_api: Api<ConfigMap> = Api::namespaced(client.clone(), &namespace);
        let deploy_api: Api<Deployment> = Api::namespaced(client.clone(), &namespace);
        let node_api: Api<Node> = Api::all(client.clone());
        let pod_api: Api<Pod> = Api::namespaced(client.clone(), &namespace);
        let secret_api: Api<Secret> = Api::namespaced(client.clone(), &namespace);
        let service_api: Api<Service> = Api::namespaced(client.clone(), &namespace);
        // return the built cluster
        Ok(ClusterMeta {
            name: name,
            client: client.clone(),
            namespace,
            cluster: cluster.clone(),
            cm_api,
            deploy_api,
            node_api,
            pod_api,
            secret_api,
            service_api,
        })
    }
}

use k8s_openapi::api::core::v1::Service;
use kube::api::Api;

/// Wrapper for node api routes in k8s
pub struct Services {
    /// API client for building namespaced clients
    _client: kube::Client,
}

impl Services {
    /// Build new wrapper for k8s functions regarding services
    ///
    /// # Arguments
    ///
    /// * `client` - Kubernetes client
    pub fn new(client: &kube::Client) -> Self {
        Services {
            _client: client.clone(),
        }
    }

    /// get service
    #[allow(dead_code)]
    pub async fn get(&self, ns: &str, name: &str) -> Result<Service, kube::Error> {
        // create namespaced services client
        let api: Api<Service> = Api::namespaced(self._client.clone(), ns);
        // get service
        api.get(name).await
    }
}

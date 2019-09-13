use k8s_openapi::api::core::v1::Service;
use kube::api::{DeleteParams, Patch, PatchParams, PostParams};
use serde_json::Value;
use thorium::Error;

use super::clusters::ClusterMeta;

/// Build a Thorium API service template
///
/// This creates an API service JSON template that can be used to create a
/// kubernetes application service for the Thorium API.
///
///  Arguments
///
/// * `meta` - Thorium cluster client and metadata
async fn api_service_template(meta: &ClusterMeta) -> Option<Value> {
    let api_spec = meta.cluster.get_api_spec();

    return match api_spec {
        Some(_api_spec) => Some(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {
                "name": "thorium-api"
            },
            "spec": {
                "selector": {
                    "app": "api"
                },
                "ports": [
                    {
                        "name": "web",
                        "port": 80,
                        "targetPort": 80
                    }
                ],
                "type": "ClusterIP"
            }
        })),
        None => None,
    };
}

/// Create an API service from a template
///
/// This creates an API service so network traffic can be routed to the API
/// from internal or external locations (using an ingress proxy like Traefik).
///
///  Arguments
///
/// * `meta` - Thorium cluster client and metadata
pub async fn create_or_update(meta: &ClusterMeta) -> Result<(), Error> {
    let params = PostParams::default();
    // build API service template
    let template = api_service_template(meta).await;
    if let Some(service_template) = template {
        let service: Service = serde_json::from_value(service_template)?;
        return match meta.service_api.create(&params, &service).await {
            Ok(_) => {
                println!(
                    "Thorium API service created in namespace {}",
                    &meta.namespace
                );
                Ok(())
            }
            Err(kube::Error::Api(error)) => {
                // do not panic if deployment exists
                if error.reason == "AlreadyExists" {
                    let patch = serde_json::json!({
                        "spec": service.spec
                    });
                    let patch = Patch::Merge(&patch);
                    let params: PatchParams = PatchParams::default();
                    return match meta.service_api.patch("thorium-api", &params, &patch).await {
                        Ok(_) => {
                            println!("Patched API service in namespace {}", &meta.namespace);
                            Ok(())
                        }
                        Err(error) => Err(Error::new(format!(
                            "Failed to patch API service in namespace {}: {}",
                            &meta.namespace, error
                        ))),
                    };
                } else {
                    Err(Error::new(format!(
                        "Failed to create API service in namespace {}: {}",
                        &meta.namespace, error
                    )))
                }
            }
            Err(error) => Err(Error::new(format!(
                "Failed to create API service in namespace {}: {}",
                &meta.namespace, error
            ))),
        };
    }
    Ok(())
}

/// Cleanup Thorium API service
///
/// This deletes a Thorium API service from kubernetes based on the default
/// service name thorium-api.
///
///  Arguments
///
/// * `meta` - Thorium cluster client and metadata
pub async fn delete(meta: &ClusterMeta) -> Result<(), Error> {
    let params: DeleteParams = DeleteParams::default();
    // delete the Thorium service
    let service_name = "thorium-api".to_string();
    match meta.service_api.delete(&service_name, &params).await {
        Ok(_) => println!("Deleted {} service", &service_name),
        Err(kube::Error::Api(error)) => {
            // service was not found, continue on
            if error.code == 404 {
                println!("Service {} does not exist, skipping deletion", service_name);
                return Ok(());
            }
            return Err(Error::new(format!(
                "Could not delete {} service: {}",
                service_name, error.message
            )));
        }
        Err(error) => {
            return Err(Error::new(format!(
                "Could not delete {} service: {}",
                service_name, error
            )))
        }
    }
    Ok(())
}

use thorium::Error;

use crate::k8s::clusters::ClusterMeta;

/// Upgrade handler for cluster version changes
pub async fn handler(_meta: &ClusterMeta) -> Result<(), Error> {
    println!("Upgrading thorium version... this is just a stub");
    Ok(())
}

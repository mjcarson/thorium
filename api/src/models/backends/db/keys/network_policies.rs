use crate::utils::Shared;

/// The keys to use to access network policy data in Redis
pub struct NetworkPolicyKeys {}

impl NetworkPolicyKeys {
    /// Builds key to the set of network policies that have had data
    /// added to Redis in the group
    ///
    /// # Arguments
    ///
    /// * `group` - The group the network policy is in
    /// * `shared` - Shared Thorium objects
    pub fn netpols(group: &str, shared: &Shared) -> String {
        format!(
            "{ns}:netpols:{group}",
            ns = shared.config.thorium.namespace,
            group = group,
        )
    }

    /// Builds key to set of images using this network policy
    ///
    /// # Arguments
    ///
    /// * `group` - The group the network policy is in
    /// * `policy_name` - The name of this network policy
    /// * `shared` - Shared Thorium objects
    pub fn used_by(group: &str, policy_name: &str, shared: &Shared) -> String {
        format!(
            "{ns}:netpol_used_by:{group}:{policy_name}",
            ns = shared.config.thorium.namespace,
            group = group,
            policy_name = policy_name,
        )
    }
}

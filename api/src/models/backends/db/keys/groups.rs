use crate::utils::Shared;

/// Keys to use to access group data/sets
pub struct GroupKeys {
    /// The key to store/retrieve group data at
    pub data: String,
    /// The key to the set of group names
    pub set: String,
    /// The key to the combined owners set
    pub combined_owners: String,
    /// The key to the combined managers set
    pub combined_managers: String,
    /// The key to the combined users set
    pub combined_users: String,
    /// The key to the combined monitors set
    pub combined_monitors: String,
    /// The key to the direct owners set
    pub direct_owners: String,
    /// The key to the direct managers set
    pub direct_managers: String,
    /// The key to the direct users set
    pub direct_users: String,
    /// The key to the direct monitors set
    pub direct_monitors: String,
    /// The key to the set of metagroups to sync owners from
    pub metagroups_owners: String,
    /// The key to the set of metagroups to sync managers from
    pub metagroups_managers: String,
    /// The key to the set of metagroups to sync users from
    pub metagroups_users: String,
    /// The key to the set of metagroups to sync monitors from
    pub metagroups_monitors: String,
}

impl GroupKeys {
    /// Builds the keys to access group data/sets in redis from a group name
    ///
    /// # Arguments
    ///
    /// * `group` - group name to build keys for
    /// * `shared` - Shared Thorium objects
    pub fn new(group: &str, shared: &Shared) -> Self {
        // build key to store group data at
        let data = Self::data(group, shared);
        // build key to group set
        let set = Self::set(shared);
        // build key to the combined member sets
        let combined_owners = Self::combined(group, "owners", shared);
        let combined_managers = Self::combined(group, "managers", shared);
        let combined_users = Self::combined(group, "users", shared);
        let combined_monitors = Self::combined(group, "monitors", shared);
        // build key to the direct member sets
        let direct_owners = Self::direct(group, "owners", shared);
        let direct_managers = Self::direct(group, "managers", shared);
        let direct_users = Self::direct(group, "users", shared);
        let direct_monitors = Self::direct(group, "monitors", shared);
        // build key to the metagroup sets
        let metagroups_owners = Self::metagroups(group, "owners", shared);
        let metagroups_managers = Self::metagroups(group, "managers", shared);
        let metagroups_users = Self::metagroups(group, "users", shared);
        let metagroups_monitors = Self::metagroups(group, "monitors", shared);
        // build key object
        GroupKeys {
            data,
            set,
            combined_owners,
            combined_managers,
            combined_users,
            combined_monitors,
            direct_owners,
            direct_managers,
            direct_users,
            direct_monitors,
            metagroups_owners,
            metagroups_managers,
            metagroups_users,
            metagroups_monitors,
        }
    }

    /// Builds key to group data
    ///
    /// # Arguments
    ///
    /// * `group` - The name of the group
    /// * `shared` - Shared Thorium objects
    pub fn data(group: &str, shared: &Shared) -> String {
        format!(
            "{ns}:groups_data:{group}",
            ns = shared.config.thorium.namespace,
            group = group
        )
    }

    /// Builds key to the combined set of group members for this role
    ///
    /// # Arguments
    ///
    /// * `group` - The group this list is for
    /// * `role` - The role this list pertains to
    /// * `shared` - Shared Thorium objects
    pub fn combined(group: &str, role: &str, shared: &Shared) -> String {
        format!(
            "{ns}:groups_combined:{group}:{role}",
            ns = shared.config.thorium.namespace,
            group = group,
            role = role
        )
    }

    /// Builds key to the direct member for this role
    ///
    /// # Arguments
    ///
    /// * `group` - The group this list is for
    /// * `role` - The role this list pertains to
    /// * `shared` - Shared Thorium objects
    pub fn direct(group: &str, role: &str, shared: &Shared) -> String {
        format!(
            "{ns}:groups_direct:{group}:{role}",
            ns = shared.config.thorium.namespace,
            group = group,
            role = role
        )
    }

    /// Builds key to the metagroups tied to a specific role for a group
    ///
    /// # Arguments
    ///
    /// * `group` - The group this list is for
    /// * `role` - The role this list pertains to
    /// * `shared` - Shared Thorium objects
    pub fn metagroups(group: &str, role: &str, shared: &Shared) -> String {
        format!(
            "{ns}:groups_metagroups:{group}:{role}",
            ns = shared.config.thorium.namespace,
            group = group,
            role = role
        )
    }

    /// Builds ket to groups set
    ///
    /// # Arguments
    ///
    /// * `shared` - Shared Thorium objects
    pub fn set(shared: &Shared) -> String {
        format!("{ns}:groups", ns = shared.config.thorium.namespace)
    }
}

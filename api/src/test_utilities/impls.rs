//! Contains trait implementations necessary for testing but not used anywhere else

use crate::models::{
    Group, GroupRequest, Image, ImageRequest, NetworkPolicy, NetworkPolicyRequest,
    NetworkPolicyRule, NetworkPolicyRuleRaw, NetworkPolicyUpdate, Pipeline, PipelineRequest,
};
use crate::{
    matches_adds, matches_adds_iter, matches_clear, matches_clear_vec_opt, matches_removes,
    matches_removes_iter, matches_update, matches_vec, same,
};

impl PartialEq<Group> for GroupRequest {
    /// Check if a [`Group`] corresponds to a [`GroupRequest`]
    ///
    /// # Arguments
    ///
    /// * `group` - The `Group` to compare against
    fn eq(&self, group: &Group) -> bool {
        // make sure the name is the same
        same!(group.name, self.name);
        // make sure user types are the same
        same!(group.owners, self.owners);
        same!(group.managers, self.managers);
        same!(group.users, self.users);
        same!(group.monitors, self.monitors);
        same!(group.description, self.description);
        true
    }
}

impl PartialEq<Image> for ImageRequest {
    /// Check if an [`Image`] corresponds to an [`ImageRequest`]
    ///
    /// # Arguments
    ///
    /// * `image` - The `Image` to compare against
    fn eq(&self, image: &Image) -> bool {
        // make sure all fields are the same
        same!(image.name, self.name);
        same!(image.group, self.group);
        same!(&image.version, &self.version);
        same!(image.scaler, self.scaler);
        same!(image.image, self.image);
        same!(&image.lifetime, &self.lifetime);
        same!(image.timeout, self.timeout);
        same!(image.resources, self.resources);
        same!(image.spawn_limit, self.spawn_limit);
        same!(image.env, self.env);
        matches_vec!(&image.volumes, &self.volumes);
        same!(image.description, self.description);
        matches_update!(image.security_context, self.security_context);
        same!(image.collect_logs, self.collect_logs);
        same!(image.generator, self.generator);
        same!(image.dependencies, self.dependencies);
        same!(image.display_type, self.display_type);
        same!(image.output_collection, self.output_collection);
        same!(image.child_filters, self.child_filters);
        same!(image.network_policies, self.network_policies);
        true
    }
}

impl PartialEq<Pipeline> for PipelineRequest {
    /// Check if a [`Pipeline`] corresponds to a [`PipelineRequest`]
    ///
    /// # Arguments
    ///
    /// * `pipe` - The `Pipeline` to compare against
    fn eq(&self, pipe: &Pipeline) -> bool {
        // make sure all fields are the same
        same!(pipe.name, self.name);
        same!(pipe.group, self.group);
        same!(self.compare_order(&pipe.order), true);
        same!(&pipe.sla, self.sla.as_ref().unwrap_or(&604_800));
        same!(&pipe.triggers, &self.triggers);
        same!(&pipe.description, &self.description);
        true
    }
}

impl PartialEq<NetworkPolicyRuleRaw> for NetworkPolicyRule {
    /// Checks if all the info in a [`NetworkPolicyRuleRaw`] was set for a [`NetworkPolicyRule`]
    ///
    /// The only "gotcha" is that the conversion from `NetworkPolicyRuleRaw` to
    /// `NetworkPolicyRule` is fallible, so if a raw rule is invalid,
    /// the two are not equal by definition
    ///
    /// # Arguments
    ///
    /// * `raw_rule` - The raw rule to compare against
    fn eq(&self, raw_rule: &NetworkPolicyRuleRaw) -> bool {
        // try casting the raw rule
        let cast_result = NetworkPolicyRule::try_from(raw_rule.clone());
        match cast_result {
            Ok(mut cast) => {
                // set our id to the rule's id
                cast.id = self.id;
                // compare everything now that we have the same ID's
                cast == *self
            }
            // if the rule is invalid, the two are not equal by definition
            Err(_) => false,
        }
    }
}

/// Check that the optional rules in a [`NetworkPolicy`] and a
/// [`NetworkPolicyRequest`] match
macro_rules! rules_opts_match {
    ($policy_rules:expr, $req_rules:expr) => {
        // first compare rules
        match (&$policy_rules, &$req_rules) {
            // they're both None so move on
            (None, None) => (),
            // one is Some and the other is None so they don't match
            (None, Some(_)) | (Some(_), None) => return false,
            // compare all rules if they're both Some
            (Some(pol_rules), Some(req_rules)) => {
                if pol_rules != req_rules {
                    return false;
                }
            }
        }
    };
}

impl PartialEq<NetworkPolicyRequest> for NetworkPolicy {
    /// Checks if all the info in a [`NetworkPolicyRequest`] was set for a [`NetworkPolicy`]
    ///
    /// The only "gotcha" is that the conversion from `NetworkPolicyRuleRaw` to
    /// `NetworkPolicyRule` is fallible, so if any of the raw rules in the request are invalid,
    /// the two are not equal by definition
    ///
    /// # Arguments
    ///
    /// * `req` - The request to compare against
    fn eq(&self, req: &NetworkPolicyRequest) -> bool {
        // check that rules match
        rules_opts_match!(self.ingress, req.ingress);
        rules_opts_match!(self.egress, req.egress);
        // make sure the groups lists are sorted the same for comparison
        let mut policy_groups = self.groups.clone();
        policy_groups.sort_unstable();
        let mut req_groups = req.groups.clone();
        req_groups.sort_unstable();
        // compare fields
        self.name == req.name
            && policy_groups == req_groups
            && self.forced_policy == req.forced_policy
    }
}

impl PartialEq<NetworkPolicyUpdate> for NetworkPolicy {
    /// Verify that all the elements in a [`NetworkPolicyUpdate`] were
    /// applied to a [`NetworkPolicy`]
    fn eq(&self, update: &NetworkPolicyUpdate) -> bool {
        matches_update!(self.name, update.new_name);
        matches_adds!(self.groups, update.add_groups);
        matches_removes!(self.groups, update.remove_groups);
        // check that we set rules to None if we wanted to allow all
        matches_clear!(self.ingress, update.clear_ingress);
        matches_clear!(self.egress, update.clear_egress);
        // check that we cleared rules (empty Vec) if we wanted to deny all
        matches_clear_vec_opt!(self.ingress, update.deny_all_ingress);
        matches_clear_vec_opt!(self.egress, update.deny_all_egress);
        // only check that rules were added if we didn't clear them
        if !update.clear_egress {
            matches_adds_iter!(self.ingress.iter().flatten(), update.add_ingress.iter());
        }
        if !update.clear_egress {
            matches_adds_iter!(self.egress.iter().flatten(), update.add_egress.iter());
        }
        // check that we removed rules
        matches_removes_iter!(
            self.ingress.iter().flatten().map(|rule| &rule.id),
            update.remove_ingress.iter()
        );
        matches_removes_iter!(
            self.egress.iter().flatten().map(|rule| &rule.id),
            update.remove_egress.iter()
        );
        matches_update!(self.forced_policy, update.forced_policy);
        matches_update!(self.default_policy, update.default_policy);
        true
    }
}

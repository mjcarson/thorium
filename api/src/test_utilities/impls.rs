//! Contains trait implementations necessary for testing but not used anywhere else

use std::collections::{BTreeSet, HashSet};
use uuid::Uuid;

use crate::models::helpers::matches_vecs_helper;
use crate::models::{
    CollectionEntity, CollectionEntityRequest, Country, DeviceEntity, DeviceEntityRequest, Entity,
    EntityMetadata, EntityMetadataRequest, EntityRequest, EntityUpdate, Group, GroupRequest, Image,
    ImageRequest, NetworkPolicy, NetworkPolicyRequest, NetworkPolicyRule, NetworkPolicyRuleRaw,
    NetworkPolicyUpdate, Pipeline, PipelineRequest, VendorEntity, VendorEntityRequest,
};
use crate::{
    matches_adds, matches_adds_iter, matches_clear, matches_clear_vec_opt, matches_removes,
    matches_removes_iter, matches_update, matches_update_opt, matches_vec, same,
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

/// Compare two serializable values by their JSON representation
///
/// This is used to compare entity metadata whose request and response types are
/// identical but which do not implement [`PartialEq`].
///
/// # Arguments
///
/// * `left` - The left value to compare
/// * `right` - The right value to compare
fn json_eq<L: serde::Serialize, R: serde::Serialize>(left: &L, right: &R) -> bool {
    // serialize both sides and compare their JSON values
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left), Ok(right)) => left == right,
        // if either side failed to serialize then they can't be equal
        _ => false,
    }
}

/// Check that a [`VendorEntity`] corresponds to a [`VendorEntityRequest`]
///
/// # Arguments
///
/// * `resp` - The vendor entity to compare against
/// * `req` - The vendor request to compare against
fn vendor_matches(resp: &VendorEntity, req: &VendorEntityRequest) -> bool {
    // convert the request's country codes into full country objects
    let mut req_countries = BTreeSet::new();
    for code in &req.countries {
        match Country::new(code) {
            Ok(country) => {
                req_countries.insert(country);
            }
            // an invalid country code means the two can't be equal
            Err(_) => return false,
        }
    }
    // compare countries and critical sectors
    resp.countries == req_countries && resp.critical_sectors == req.critical_sectors
}

/// Check that a [`DeviceEntity`] corresponds to a [`DeviceEntityRequest`]
///
/// # Arguments
///
/// * `resp` - The device entity to compare against
/// * `req` - The device request to compare against
fn device_matches(resp: &DeviceEntity, req: &DeviceEntityRequest) -> bool {
    // make sure the simple fields match
    if !matches_vecs_helper(&resp.urls, &req.urls)
        || resp.critical_system != req.critical_system
        || resp.sensitive_location != req.sensitive_location
        || resp.critical_sectors != req.critical_sectors
    {
        return false;
    }
    // the response contains full vendor entities, so compare them by their ids
    let resp_ids: BTreeSet<Uuid> = resp.vendors.iter().map(|vendor| vendor.id).collect();
    let req_ids: BTreeSet<Uuid> = req.vendors.iter().copied().collect();
    resp_ids == req_ids
}

/// Check that a [`CollectionEntity`] corresponds to a [`CollectionEntityRequest`]
///
/// # Arguments
///
/// * `resp` - The collection entity to compare against
/// * `req` - The collection request to compare against
fn collection_matches(resp: &CollectionEntity, req: &CollectionEntityRequest) -> bool {
    // collection kinds don't implement PartialEq so compare their str forms
    if resp.collection_kind.as_ref() != req.collection_kind.as_ref() {
        return false;
    }
    // make sure both sides have the same number of tag keys
    if resp.collection_tags.len() != req.collection_tags.len() {
        return false;
    }
    // make sure every tag key/value in the request is present in the response
    for (key, req_values) in &req.collection_tags {
        match resp.collection_tags.get(key) {
            Some(resp_values) => {
                // compare the two sets of values ignoring ordering
                let req_set: HashSet<&String> = req_values.iter().collect();
                let resp_set: HashSet<&String> = resp_values.iter().collect();
                if req_set != resp_set {
                    return false;
                }
            }
            // the response is missing a tag key from the request
            None => return false,
        }
    }
    // the request stores its flags as options that default to false in the response
    resp.tags_case_insensitive == req.tags_case_insensitive.unwrap_or(false)
        && resp.ignore_groups == req.ignore_groups.unwrap_or(false)
        && resp.start == req.start
        && resp.end == req.end
}

impl PartialEq<EntityRequest> for Entity {
    /// Check if an [`Entity`] corresponds to an [`EntityRequest`]
    ///
    /// # Arguments
    ///
    /// * `req` - The `EntityRequest` to compare against
    fn eq(&self, req: &EntityRequest) -> bool {
        // make sure the shared fields are the same
        same!(self.name, req.name);
        same!(self.kind, req.kind());
        same!(self.description, req.description);
        // the groups may be returned in a different order
        matches_vec!(&self.groups, &req.groups);
        // make sure every tag key/value in the request was applied to the entity
        for (key, values) in &req.tags {
            match self.tags.get(key) {
                Some(value_map) => {
                    for value in values {
                        if !value_map.contains_key(value) {
                            return false;
                        }
                    }
                }
                // the entity is missing a tag key from the request
                None => return false,
            }
        }
        // compare the kind-specific metadata
        match (&self.metadata, &req.metadata) {
            // these kinds carry no comparable metadata beyond their kind
            (EntityMetadata::Other, EntityMetadataRequest::Other)
            | (EntityMetadata::WindowsProcessTree(_), EntityMetadataRequest::WindowsProcessTree) => {
                true
            }
            // these kinds have distinct request/response types so compare them explicitly
            (EntityMetadata::Vendor(resp), EntityMetadataRequest::Vendor(req)) => {
                vendor_matches(resp, req)
            }
            (EntityMetadata::Device(resp), EntityMetadataRequest::Device(req)) => {
                device_matches(resp, req)
            }
            (EntityMetadata::Collection(resp), EntityMetadataRequest::Collection(req)) => {
                collection_matches(resp, req)
            }
            // these kinds share a type between request and response so compare via JSON
            (EntityMetadata::FileSystem(resp), EntityMetadataRequest::FileSystem(req)) => {
                json_eq(resp, req)
            }
            (EntityMetadata::Folder(resp), EntityMetadataRequest::Folder(req)) => json_eq(resp, req),
            (EntityMetadata::WindowsProcess(resp), EntityMetadataRequest::WindowsProcess(req)) => {
                json_eq(resp, req)
            }
            (
                EntityMetadata::NetworkConnection(resp),
                EntityMetadataRequest::NetworkConnection(req),
            ) => json_eq(resp, req),
            (EntityMetadata::PeSection(resp), EntityMetadataRequest::PeSection(req)) => {
                json_eq(resp, req)
            }
            (EntityMetadata::PeImport(resp), EntityMetadataRequest::PeImport(req)) => {
                json_eq(resp, req)
            }
            (EntityMetadata::SigmaRule(resp), EntityMetadataRequest::SigmaRule(req)) => {
                json_eq(resp, req)
            }
            (EntityMetadata::Flag(resp), EntityMetadataRequest::Flag(req)) => json_eq(resp, req),
            (EntityMetadata::Incident(resp), EntityMetadataRequest::Incident(req)) => {
                json_eq(resp, req)
            }
            (
                EntityMetadata::CompiledFunction(resp),
                EntityMetadataRequest::CompiledFunction(req),
            ) => json_eq(resp, req),
            (
                EntityMetadata::DecompiledFunction(resp),
                EntityMetadataRequest::DecompiledFunction(req),
            ) => json_eq(resp, req),
            // any mismatched pairing of kinds means they aren't equal
            _ => false,
        }
    }
}

impl PartialEq<EntityUpdate> for Entity {
    /// Verify that all the elements in an [`EntityUpdate`] were applied to an [`Entity`]
    ///
    /// # Arguments
    ///
    /// * `update` - The `EntityUpdate` to verify was applied
    fn eq(&self, update: &EntityUpdate) -> bool {
        // check that the name was updated if requested
        matches_update!(self.name, update.name);
        // check that the description was cleared or updated as requested
        matches_clear!(self.description, update.clear_description);
        if !update.clear_description {
            matches_update_opt!(self.description, update.description);
        }
        // check that the groups were added and removed as requested
        matches_adds!(self.groups, update.add_groups);
        matches_removes!(self.groups, update.remove_groups);
        true
    }
}

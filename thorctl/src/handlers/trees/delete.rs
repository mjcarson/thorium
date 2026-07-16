//! Delete the descendant entities under a set of initial tree nodes

use std::collections::{HashMap, HashSet, VecDeque};

use colored::Colorize;
use futures::stream::{self, StreamExt};
use thorium::Thorium;
use thorium::models::{Directionality, EntityKinds, Tree, TreeNode};
use uuid::Uuid;

use crate::Args;
use crate::Error;
use crate::args::trees::DeleteTree;
use crate::handlers::progress::{Bar, BarKind};

/// A traversal view of a [`Tree`] used to plan which entities to delete
///
/// This is intentionally decoupled from [`Tree`] so the deletion logic can be
/// unit tested without building full [`Tree`]/[`thorium::models::Entity`] values.
struct DeleteGraph {
    /// The hashes of the initial (root) nodes, which are never deleted
    initial: HashSet<u64>,
    /// A map of node hash to entity id for every entity node in the tree
    entities: HashMap<u64, Uuid>,
    /// A map of node hash to the hashes of its parents
    parents_of: HashMap<u64, HashSet<u64>>,
    /// A map of node hash to the hashes of its children
    children_of: HashMap<u64, HashSet<u64>>,
}

impl DeleteGraph {
    /// Build a [`DeleteGraph`] from a fully grown [`Tree`]
    ///
    /// # Arguments
    ///
    /// * `tree` - The tree to build a traversal view from
    fn from_tree(tree: &Tree) -> Self {
        // collect the hashes of our initial root nodes
        let initial = tree.initial.iter().copied().collect::<HashSet<u64>>();
        // map every entity node's hash to its entity id
        let mut entities = HashMap::new();
        for (hash, node) in &tree.data_map {
            // only entity nodes are ever deletable
            if let TreeNode::Entity(entity) = node {
                // record this entity's id keyed by its node hash
                entities.insert(*hash, entity.id);
            }
        }
        // build the parent/child adjacency maps for our tree
        let mut parents_of: HashMap<u64, HashSet<u64>> = HashMap::new();
        let mut children_of: HashMap<u64, HashSet<u64>> = HashMap::new();
        // fold in both displayed branches and hinted branches so no real edge is missed
        for branch_map in [&tree.branches, &tree.hint_branches] {
            // step over every source node and its branches
            for (src, branches) in branch_map {
                // convert each branch into a directed parent/child edge
                for branch in branches {
                    match branch.direction {
                        // a `To` branch points from a parent (src) down to a child
                        Directionality::To => {
                            children_of.entry(*src).or_default().insert(branch.node);
                            parents_of.entry(branch.node).or_default().insert(*src);
                        }
                        // a `From` branch points from a child (src) up to a parent
                        Directionality::From => {
                            parents_of.entry(*src).or_default().insert(branch.node);
                            children_of.entry(branch.node).or_default().insert(*src);
                        }
                        // treat bidirectional edges as a mutual parent relationship only so
                        // neither endpoint is ever traversed into as a descendant and each
                        // protects the other from deletion (conservative for a destructive op)
                        Directionality::Bidirectional => {
                            parents_of.entry(*src).or_default().insert(branch.node);
                            parents_of.entry(branch.node).or_default().insert(*src);
                        }
                    }
                }
            }
        }
        DeleteGraph {
            initial,
            entities,
            parents_of,
            children_of,
        }
    }

    /// Plan which entity nodes should be deleted, recording a reason for each
    ///
    /// An entity is deleted only if every one of its parents is an initial node
    /// or is itself being deleted; any entity with a surviving/external parent is
    /// preserved, as are all of its descendants. The returned [`Plan`] also
    /// records a [`Decision`] for every entity node so callers can explain the
    /// outcome (used by `--debug`).
    fn plan(&self) -> Plan {
        // compute the downward closure of our initial nodes by following child edges
        let mut down: HashSet<u64> = self.initial.iter().copied().collect();
        // seed our traversal queue with the initial nodes
        let mut queue: VecDeque<u64> = self.initial.iter().copied().collect();
        // walk down the tree adding every reachable descendant
        while let Some(node) = queue.pop_front() {
            // follow this node's child edges
            if let Some(children) = self.children_of.get(&node) {
                // add each newly discovered child to our closure
                for child in children {
                    if down.insert(*child) {
                        queue.push_back(*child);
                    }
                }
            }
        }
        // our candidates are descendant entities that are not initial nodes
        let candidates: HashSet<u64> = down
            .iter()
            .copied()
            .filter(|hash| !self.initial.contains(hash) && self.entities.contains_key(hash))
            .collect();
        // track which candidates are protected and why
        let mut protected: HashSet<u64> = HashSet::new();
        // record the surviving parents that directly protect a candidate
        let mut external: HashMap<u64, Vec<u64>> = HashMap::new();
        // record the preserved ancestor that propagated protection to a candidate
        let mut protected_by: HashMap<u64, u64> = HashMap::new();
        // track newly protected nodes so we can propagate protection to their descendants
        let mut work: VecDeque<u64> = VecDeque::new();
        // seed protection from candidates with a surviving/external parent
        for candidate in &candidates {
            // check this candidate's parents for anything that will survive
            if let Some(parents) = self.parents_of.get(candidate) {
                // collect the parents that are neither a root nor another candidate
                let surviving = parents
                    .iter()
                    .copied()
                    .filter(|parent| !self.initial.contains(parent) && !candidates.contains(parent))
                    .collect::<Vec<u64>>();
                // protect this candidate if it hangs off a surviving parent
                if !surviving.is_empty() {
                    // mark this candidate protected and record the surviving parents
                    protected.insert(*candidate);
                    external.insert(*candidate, surviving);
                    // queue it so its descendants inherit protection
                    work.push_back(*candidate);
                }
            }
        }
        // propagate protection downward so descendants of a preserved node are also preserved
        while let Some(node) = work.pop_front() {
            // walk this protected node's children
            if let Some(children) = self.children_of.get(&node) {
                // protect any candidate child that isn't already protected
                for child in children {
                    if candidates.contains(child) && protected.insert(*child) {
                        // record which ancestor propagated protection to this child
                        protected_by.insert(*child, node);
                        work.push_back(*child);
                    }
                }
            }
        }
        // build a decision for every entity node and collect the ones to delete
        let mut decisions = HashMap::with_capacity(self.entities.len());
        let mut to_delete = Vec::new();
        for hash in self.entities.keys().copied() {
            // classify this entity node in priority order
            let decision = if self.initial.contains(&hash) {
                // initial nodes are roots and are never deleted
                Decision::Initial
            } else if !down.contains(&hash) {
                // this entity can't be reached by traversing down from any initial node
                Decision::Unreachable
            } else if let Some(surviving) = external.get(&hash) {
                // this entity is held by one or more surviving parents
                Decision::ExternalParent(surviving.clone())
            } else if let Some(ancestor) = protected_by.get(&hash) {
                // this entity descends from a preserved node
                Decision::ProtectedDescendant(*ancestor)
            } else {
                // nothing is keeping this entity so it will be deleted
                to_delete.push(hash);
                Decision::Delete
            };
            // record this entity's decision
            decisions.insert(hash, decision);
        }
        Plan {
            to_delete,
            decisions,
        }
    }
}

/// Why an entity node was or wasn't scheduled for deletion
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    /// This entity is scheduled for deletion
    Delete,
    /// This entity is an initial (root) node and is never deleted
    Initial,
    /// This entity is not reachable downward from any initial node
    Unreachable,
    /// This entity is preserved because it has surviving/external parent(s)
    ///
    /// Holds the node hashes of the surviving parents.
    ExternalParent(Vec<u64>),
    /// This entity is preserved because it descends from a preserved node
    ///
    /// Holds the node hash of the preserved ancestor that protected it.
    ProtectedDescendant(u64),
}

/// The result of planning deletions over a tree
struct Plan {
    /// The hashes of the entity nodes to delete
    to_delete: Vec<u64>,
    /// The decision for every entity node in the tree, keyed by node hash
    decisions: HashMap<u64, Decision>,
}

/// A single entity slated for deletion
struct DeleteTarget {
    /// The id of the entity to delete
    id: Uuid,
    /// The name of the entity to delete
    name: String,
    /// The kind of entity to delete
    kind: EntityKinds,
}

/// Print a summary of the entities that will be deleted
///
/// # Arguments
///
/// * `targets` - The entities slated for deletion
fn print_summary(targets: &[DeleteTarget]) {
    // let the user know if there is nothing to delete
    if targets.is_empty() {
        println!("{}", "No entities to delete".bright_yellow());
        return;
    }
    // print a header for our delete list
    println!(
        "The following {} entities will be deleted:",
        targets.len().to_string().bright_red()
    );
    // print a line for each entity we plan to delete
    for target in targets {
        println!(
            "  {} {} ({})",
            target.id.to_string().bright_red(),
            target.name,
            target.kind
        );
    }
}

/// Describe a tree node by its hash for debug output
///
/// # Arguments
///
/// * `tree` - The tree to resolve the node from
/// * `hash` - The hash of the node to describe
fn describe_node(tree: &Tree, hash: u64) -> String {
    // resolve this hash into a readable label from the tree's node data
    match tree.data_map.get(&hash) {
        Some(TreeNode::Entity(entity)) => format!("entity '{}' ({})", entity.name, entity.id),
        Some(TreeNode::Sample(sample)) => format!("sample {}", sample.sha256),
        Some(TreeNode::Repo(repo)) => format!("repo {}", repo.url),
        Some(TreeNode::Tag(_)) => format!("tag node #{hash}"),
        None => format!("unknown node #{hash}"),
    }
}

/// Log why each entity in the tree is or isn't being deleted
///
/// # Arguments
///
/// * `tree` - The tree that was built and planned over
/// * `graph` - The traversal view used to plan deletions
/// * `plan` - The deletion plan with a decision for every entity node
fn debug_report(tree: &Tree, graph: &DeleteGraph, plan: &Plan) {
    // build a dimmed prefix so debug lines are easy to spot
    let prefix = "debug:".dimmed();
    // tally the node kinds in the built tree
    let (mut samples, mut repos, mut tags) = (0usize, 0usize, 0usize);
    for node in tree.data_map.values() {
        match node {
            TreeNode::Entity(_) => (),
            TreeNode::Sample(_) => samples += 1,
            TreeNode::Repo(_) => repos += 1,
            TreeNode::Tag(_) => tags += 1,
        }
    }
    // count how many initial nodes actually resolved into the tree's node data
    let resolved_initial = tree
        .initial
        .iter()
        .filter(|hash| tree.data_map.contains_key(hash))
        .count();
    // count the total number of displayed and hinted branch edges
    let branches: usize = tree.branches.values().map(std::collections::HashSet::len).sum();
    let hint_branches: usize = tree
        .hint_branches
        .values()
        .map(std::collections::HashSet::len)
        .sum();
    // print the tree level stats
    println!(
        "{prefix} tree has {} nodes ({} entities, {samples} samples, {repos} repos, {tags} tags), \
         {} initial nodes ({resolved_initial} resolved), {branches} branches, {hint_branches} hint branches",
        tree.data_map.len(),
        graph.entities.len(),
        tree.initial.len(),
    );
    // build a stable, name sorted view of every entity decision
    let mut lines = plan
        .decisions
        .iter()
        .map(|(hash, decision)| {
            // resolve this entity's name for sorting and display
            let name = match tree.data_map.get(hash) {
                Some(TreeNode::Entity(entity)) => entity.name.clone(),
                _ => format!("#{hash}"),
            };
            (name, *hash, decision)
        })
        .collect::<Vec<(String, u64, &Decision)>>();
    // sort by entity name for readable output
    lines.sort_by(|left, right| left.0.cmp(&right.0));
    // print a decision line for every entity node in the tree
    for (name, hash, decision) in lines {
        // count this entity's parents and children to reveal direction/orientation issues
        let parents = graph.parents_of.get(&hash).map_or(0, HashSet::len);
        let children = graph.children_of.get(&hash).map_or(0, HashSet::len);
        // build the human readable reason for this decision
        let reason = match decision {
            Decision::Delete => "DELETE".bright_red().to_string(),
            Decision::Initial => "KEEP: initial node".to_string(),
            Decision::Unreachable => {
                "KEEP: not reachable downward from initial nodes".to_string()
            }
            Decision::ExternalParent(surviving) => {
                // list each surviving parent that protects this entity
                let parents = surviving
                    .iter()
                    .map(|parent| describe_node(tree, *parent))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("KEEP: surviving parent(s): {parents}")
            }
            Decision::ProtectedDescendant(ancestor) => {
                format!(
                    "KEEP: descends from preserved {}",
                    describe_node(tree, *ancestor)
                )
            }
        };
        // print this entity's decision line
        println!("{prefix} entity '{name}' [parents={parents}, children={children}] -> {reason}");
    }
}

/// Delete the descendant entities under a set of initial tree nodes
///
/// # Arguments
///
/// * `thorium` - The Thorium client
/// * `args` - The top level Thorctl args
/// * `cmd` - The delete tree command to execute
pub async fn delete(thorium: &Thorium, args: &Args, cmd: &DeleteTree) -> Result<(), Error> {
    // build the query and opts for the tree we want to prune
    let query = cmd.to_query()?;
    let opts = cmd.to_opts();
    // build the tree from our initial nodes
    let tree = thorium.trees.start(&opts, &query).await?;
    // plan which entity nodes should be deleted
    let graph = DeleteGraph::from_tree(&tree);
    let plan = graph.plan();
    // log why each entity is or isn't being deleted if debugging is enabled
    if cmd.debug {
        debug_report(&tree, &graph, &plan);
    }
    // resolve each planned node hash back into its entity info for deletion and display
    let mut targets = Vec::with_capacity(plan.to_delete.len());
    for hash in &plan.to_delete {
        // only entity nodes should have made it into our plan
        if let Some(TreeNode::Entity(entity)) = tree.data_map.get(hash) {
            // record the info we need to delete and display this entity
            targets.push(DeleteTarget {
                id: entity.id,
                name: entity.name.clone(),
                kind: entity.kind,
            });
        }
    }
    // sort our targets by name for stable, readable output
    targets.sort_by(|left, right| left.name.cmp(&right.name));
    // show the user what we plan to delete
    print_summary(&targets);
    // stop here if this is only a preview or there is nothing to delete
    if cmd.dry_run || targets.is_empty() {
        return Ok(());
    }
    // confirm the deletion unless the user opted to skip the prompt
    if !cmd.force {
        // ask the user to confirm this destructive action
        let confirmed = dialoguer::Confirm::new()
            .with_prompt(format!("Delete {} entities?", targets.len()))
            .default(false)
            .interact()?;
        // abort if the user declined
        if !confirmed {
            println!("Aborted");
            return Ok(());
        }
    }
    // build a bounded progress bar to track our deletions
    let bar = Bar::new("Deleting entities", "", BarKind::Bound(targets.len() as u64));
    // delete each entity concurrently, letting the cascade clean up its links
    stream::iter(&targets)
        .map(|target| async {
            // delete this entity and its associations
            if let Err(err) = thorium.entities.delete(target.id).await {
                // log the failure without aborting the rest of the run
                bar.error(format!(
                    "Failed to delete entity {} ({}): {}",
                    target.name, target.id, err
                ));
            }
            // count this entity as processed
            bar.inc(1);
        })
        .buffer_unordered(args.workers)
        .collect::<Vec<()>>()
        .await;
    // finish our progress bar
    bar.finish_with_message("✅");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a [`DeleteGraph`] from a list of initial nodes, entity nodes, and edges
    ///
    /// # Arguments
    ///
    /// * `initial` - The hashes of the initial root nodes
    /// * `entity_hashes` - The hashes of nodes that are entities
    /// * `edges` - The directed `(parent, child)` edges in the tree
    fn graph(initial: &[u64], entity_hashes: &[u64], edges: &[(u64, u64)]) -> DeleteGraph {
        // collect our initial root hashes
        let initial = initial.iter().copied().collect::<HashSet<u64>>();
        // give every entity node a synthetic id
        let mut entities = HashMap::new();
        for hash in entity_hashes {
            entities.insert(*hash, Uuid::new_v4());
        }
        // build our parent/child adjacency from the given edges
        let mut parents_of: HashMap<u64, HashSet<u64>> = HashMap::new();
        let mut children_of: HashMap<u64, HashSet<u64>> = HashMap::new();
        for (parent, child) in edges {
            children_of.entry(*parent).or_default().insert(*child);
            parents_of.entry(*child).or_default().insert(*parent);
        }
        DeleteGraph {
            initial,
            entities,
            parents_of,
            children_of,
        }
    }

    /// Sort a list of hashes so plan results can be compared deterministically
    ///
    /// # Arguments
    ///
    /// * `hashes` - The hashes to sort
    fn sorted(mut hashes: Vec<u64>) -> Vec<u64> {
        hashes.sort_unstable();
        hashes
    }

    // node label constants used across the tests
    const A: u64 = 1;
    const B: u64 = 2;
    const C: u64 = 3;
    const D: u64 = 4;
    const E: u64 = 5;
    const X: u64 = 6;
    const S: u64 = 7;
    const F: u64 = 8;

    /// A child with an external parent (and its descendants) must be preserved
    #[test]
    fn external_parent_protects_node() {
        // A -> B -> C, external D -> C, C -> E
        let graph = graph(
            &[A],
            &[B, C, D, E],
            &[(A, B), (B, C), (D, C), (C, E)],
        );
        // only B is safe to delete; C is held by D and E hangs off the preserved C
        assert_eq!(sorted(graph.plan().to_delete), vec![B]);
    }

    /// A plain chain rooted at an initial node deletes every descendant
    #[test]
    fn plain_chain_deletes_all_descendants() {
        // A -> B -> C -> D
        let graph = graph(&[A], &[B, C, D], &[(A, B), (B, C), (C, D)]);
        // every descendant entity is deleted, but the initial node A is not
        assert_eq!(sorted(graph.plan().to_delete), vec![B, C, D]);
    }

    /// A node whose parents are all in scope is deletable even via a diamond
    #[test]
    fn diamond_with_in_scope_parents_deletes_all() {
        // A -> B -> C and A -> X -> C
        let graph = graph(&[A], &[B, X, C], &[(A, B), (A, X), (B, C), (X, C)]);
        // C's parents (B and X) are both being deleted, so C is deletable too
        assert_eq!(sorted(graph.plan().to_delete), vec![B, C, X]);
    }

    /// Protection propagates down through multiple descendant levels
    #[test]
    fn protection_propagates_through_descendants() {
        // A -> B -> C, external D -> C, C -> E -> F
        let graph = graph(
            &[A],
            &[B, C, D, E, F],
            &[(A, B), (B, C), (D, C), (C, E), (E, F)],
        );
        // C is protected by D, and protection flows down to E and F
        assert_eq!(sorted(graph.plan().to_delete), vec![B]);
    }

    /// A surviving non-entity node in the middle protects its entity child
    #[test]
    fn surviving_sample_protects_child() {
        // A -> S (sample, not an entity) -> B
        let graph = graph(&[A], &[B], &[(A, S), (S, B)]);
        // S survives (it is never deleted) so its child B is preserved
        assert!(graph.plan().to_delete.is_empty());
    }

    /// Initial nodes are never deleted even if they are entities
    #[test]
    fn initial_nodes_are_never_deleted() {
        // A (entity) is the initial node and parents B
        let graph = graph(&[A], &[A, B], &[(A, B)]);
        // only the child B is deleted; the initial A is left alone
        assert_eq!(sorted(graph.plan().to_delete), vec![B]);
    }

    /// The plan records an accurate reason for every entity decision
    #[test]
    fn plan_records_reasons() {
        // A -> B -> C, external D -> C, C -> E, plus a disconnected entity Z
        let graph = graph(
            &[A],
            &[B, C, D, E, F],
            &[(A, B), (B, C), (D, C), (C, E)],
        );
        // build the plan so we can inspect its per entity decisions
        let plan = graph.plan();
        // B has only in-scope parents so it is deleted
        assert_eq!(plan.decisions.get(&B), Some(&Decision::Delete));
        // C is preserved because it has a surviving external parent D
        assert_eq!(
            plan.decisions.get(&C),
            Some(&Decision::ExternalParent(vec![D]))
        );
        // E is preserved because it descends from the preserved C
        assert_eq!(
            plan.decisions.get(&E),
            Some(&Decision::ProtectedDescendant(C))
        );
        // F is an entity with no path from the initial node so it is unreachable
        assert_eq!(plan.decisions.get(&F), Some(&Decision::Unreachable));
    }
}

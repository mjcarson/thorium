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

    /// Plan which entity nodes should be deleted
    ///
    /// Returns the hashes of the entity nodes to delete. An entity is deleted
    /// only if every one of its parents is an initial node or is itself being
    /// deleted; any entity with a surviving/external parent is preserved, as are
    /// all of its descendants.
    fn plan(&self) -> Vec<u64> {
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
        // protect any candidate that has a parent outside the deletion scope
        let mut protected: HashSet<u64> = HashSet::new();
        // track newly protected nodes so we can propagate protection to their descendants
        let mut work: VecDeque<u64> = VecDeque::new();
        // seed protection from candidates with a surviving/external parent
        for candidate in &candidates {
            // check this candidate's parents for anything that will survive
            if let Some(parents) = self.parents_of.get(candidate) {
                // a parent that is neither a root nor another candidate will survive
                let has_surviving_parent = parents
                    .iter()
                    .any(|parent| !self.initial.contains(parent) && !candidates.contains(parent));
                // protect this candidate if it hangs off a surviving parent
                if has_surviving_parent {
                    // mark this candidate protected and queue it for propagation
                    protected.insert(*candidate);
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
                        work.push_back(*child);
                    }
                }
            }
        }
        // delete every candidate that wasn't protected
        candidates
            .into_iter()
            .filter(|hash| !protected.contains(hash))
            .collect()
    }
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
    let to_delete = graph.plan();
    // resolve each planned node hash back into its entity info for deletion and display
    let mut targets = Vec::with_capacity(to_delete.len());
    for hash in &to_delete {
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
        assert_eq!(sorted(graph.plan()), vec![B]);
    }

    /// A plain chain rooted at an initial node deletes every descendant
    #[test]
    fn plain_chain_deletes_all_descendants() {
        // A -> B -> C -> D
        let graph = graph(&[A], &[B, C, D], &[(A, B), (B, C), (C, D)]);
        // every descendant entity is deleted, but the initial node A is not
        assert_eq!(sorted(graph.plan()), vec![B, C, D]);
    }

    /// A node whose parents are all in scope is deletable even via a diamond
    #[test]
    fn diamond_with_in_scope_parents_deletes_all() {
        // A -> B -> C and A -> X -> C
        let graph = graph(&[A], &[B, X, C], &[(A, B), (A, X), (B, C), (X, C)]);
        // C's parents (B and X) are both being deleted, so C is deletable too
        assert_eq!(sorted(graph.plan()), vec![B, C, X]);
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
        assert_eq!(sorted(graph.plan()), vec![B]);
    }

    /// A surviving non-entity node in the middle protects its entity child
    #[test]
    fn surviving_sample_protects_child() {
        // A -> S (sample, not an entity) -> B
        let graph = graph(&[A], &[B], &[(A, S), (S, B)]);
        // S survives (it is never deleted) so its child B is preserved
        assert!(graph.plan().is_empty());
    }

    /// Initial nodes are never deleted even if they are entities
    #[test]
    fn initial_nodes_are_never_deleted() {
        // A (entity) is the initial node and parents B
        let graph = graph(&[A], &[A, B], &[(A, B)]);
        // only the child B is deleted; the initial A is left alone
        assert_eq!(sorted(graph.plan()), vec![B]);
    }
}

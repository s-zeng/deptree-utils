use std::collections::{HashMap, HashSet};

use petgraph::algo::{dijkstra, floyd_warshall};
use petgraph::graph::NodeIndex;
use petgraph::visit::Reversed;
use petgraph::{Direction, Graph};
use serde::{Deserialize, Serialize};

pub mod dependency_graph;
pub mod filters;
pub use dependency_graph::{DependencyGraph, GraphId};

/// Graph node representation shared between the CLI and frontend.
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    #[cfg_attr(
        feature = "ts-bindings",
        ts(type = "\"module\" | \"script\" | \"namespace\" | \"namespace_group\"")
    )]
    pub node_type: String, // "module", "script", "namespace", or "namespace_group"
    pub is_orphan: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub highlighted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub parent: Option<String>,
}

/// Graph edge representation shared between the CLI and frontend.
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// Graph configuration for visualization consumers.
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub include_orphans: bool,
    pub include_namespaces: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub highlighted_modules: Option<Vec<String>>,
}

/// Complete graph data payload passed from the CLI to the frontend.
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub config: Option<GraphConfig>,
}

/// Build a petgraph graph from node/edge lists.
pub fn build_graph(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> (Graph<String, ()>, HashMap<String, NodeIndex>) {
    let mut graph = Graph::<String, ()>::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

    for node in nodes {
        let idx = graph.add_node(node.id.clone());
        node_map.insert(node.id.clone(), idx);
    }

    for edge in edges {
        if let (Some(&source_idx), Some(&target_idx)) =
            (node_map.get(&edge.source), node_map.get(&edge.target))
        {
            graph.add_edge(source_idx, target_idx, ());
        }
    }

    (graph, node_map)
}

/// Compute shortest-path distances from a single node to all reachable nodes (unit weights).
pub fn bfs_distances_from_node(
    graph: &Graph<String, ()>,
    root_idx: NodeIndex,
) -> HashMap<String, usize> {
    dijkstra(graph, root_idx, None, |_| 1usize)
        .into_iter()
        .filter_map(|(idx, cost)| graph.node_weight(idx).map(|id| (id.clone(), cost)))
        .collect()
}

/// Compute distances from all nodes to all reachable nodes.
/// Returns a map: node_id -> (reachable_node_id -> distance)
pub fn compute_all_distances(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> HashMap<String, HashMap<String, usize>> {
    let (graph, _) = build_graph(nodes, edges);
    let mut all_distances: HashMap<String, HashMap<String, usize>> = HashMap::new();

    if let Ok(floyd) = floyd_warshall(&graph, |_| 1usize) {
        for ((from_idx, to_idx), dist) in floyd {
            if let (Some(from_id), Some(to_id)) =
                (graph.node_weight(from_idx), graph.node_weight(to_idx))
            {
                all_distances
                    .entry(from_id.clone())
                    .or_default()
                    .insert(to_id.clone(), dist);
            }
        }
    }

    all_distances
}

/// Check if a node is an orphan (has no incoming or outgoing edges).
pub fn is_orphan_node(node_id: &str, edges: &[GraphEdge]) -> bool {
    let has_incoming = edges.iter().any(|e| e.target == node_id);
    let has_outgoing = edges.iter().any(|e| e.source == node_id);
    !has_incoming && !has_outgoing
}

/// Get all nodes within max_distance from any of the root nodes using a precomputed distance map.
pub fn get_nodes_within_distance(
    roots: &[String],
    max_distance: usize,
    distance_map: &HashMap<String, HashMap<String, usize>>,
) -> HashSet<String> {
    let mut result = HashSet::new();

    for root in roots {
        result.insert(root.clone());

        if let Some(distances) = distance_map.get(root) {
            for (node_id, &dist) in distances {
                if dist <= max_distance {
                    result.insert(node_id.clone());
                }
            }
        }
    }

    result
}

/// Get all upstream dependencies (nodes that the roots depend on).
pub fn get_upstream_nodes(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
) -> HashSet<String> {
    get_upstream_nodes_with_distance(roots, edges, max_distance)
        .into_keys()
        .collect()
}

/// Get all downstream dependents (nodes that depend on the roots).
pub fn get_downstream_nodes(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
) -> HashSet<String> {
    get_downstream_nodes_with_distance(roots, edges, max_distance)
        .into_keys()
        .collect()
}

/// Get upstream dependencies with distance information (root has distance 0).
pub fn get_upstream_nodes_with_distance(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
) -> HashMap<String, usize> {
    collect_reachable_with_distance(roots, edges, max_distance, Direction::Outgoing)
}

/// Get downstream dependents with distance information (root has distance 0).
pub fn get_downstream_nodes_with_distance(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
) -> HashMap<String, usize> {
    collect_reachable_with_distance(roots, edges, max_distance, Direction::Incoming)
}

fn collect_reachable_with_distance(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
    direction: Direction,
) -> HashMap<String, usize> {
    let node_ids: HashSet<String> = edges
        .iter()
        .flat_map(|e| [e.source.clone(), e.target.clone()])
        .chain(roots.iter().cloned())
        .collect();

    let graph_nodes: Vec<GraphNode> = node_ids
        .iter()
        .map(|id| GraphNode {
            id: id.clone(),
            node_type: String::new(),
            is_orphan: false,
            highlighted: None,
            parent: None,
        })
        .collect();

    let (graph, node_map) = build_graph(&graph_nodes, edges);

    let mut result: HashMap<String, usize> = HashMap::new();

    for root in roots {
        if let Some(&start_idx) = node_map.get(root) {
            let view = match direction {
                Direction::Outgoing => EitherGraph::Forward(&graph),
                Direction::Incoming => EitherGraph::Reversed(Reversed(&graph)),
            };

            for (node_idx, distance) in view.run_dijkstra(start_idx) {
                if max_distance.map(|limit| distance > limit).unwrap_or(false) {
                    continue;
                }

                if let Some(node_id) = graph.node_weight(node_idx) {
                    match result.get_mut(node_id) {
                        Some(existing) if *existing <= distance => {}
                        Some(existing) => *existing = distance,
                        None => {
                            result.insert(node_id.clone(), distance);
                        }
                    }
                }
            }
        }
    }

    result
}

enum EitherGraph<'a> {
    Forward(&'a Graph<String, ()>),
    Reversed(Reversed<&'a Graph<String, ()>>),
}

impl<'a> EitherGraph<'a> {
    fn run_dijkstra(&self, start: NodeIndex) -> HashMap<NodeIndex, usize> {
        match self {
            EitherGraph::Forward(graph) => dijkstra(*graph, start, None, |_| 1usize),
            EitherGraph::Reversed(graph) => dijkstra(*graph, start, None, |_| 1usize),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bfs_distances() {
        let mut graph = Graph::<String, ()>::new();
        let a = graph.add_node("a".to_string());
        let b = graph.add_node("b".to_string());
        let c = graph.add_node("c".to_string());

        graph.add_edge(a, b, ());
        graph.add_edge(b, c, ());

        let distances = bfs_distances_from_node(&graph, a);

        assert_eq!(distances.get("a"), Some(&0));
        assert_eq!(distances.get("b"), Some(&1));
        assert_eq!(distances.get("c"), Some(&2));
    }

    #[test]
    fn test_is_orphan() {
        let edges = vec![GraphEdge {
            source: "a".to_string(),
            target: "b".to_string(),
        }];

        assert!(!is_orphan_node("a", &edges)); // has outgoing
        assert!(!is_orphan_node("b", &edges)); // has incoming
        assert!(is_orphan_node("c", &edges)); // no edges
    }

    #[test]
    fn test_upstream_nodes() {
        let edges = vec![
            GraphEdge {
                source: "main".to_string(),
                target: "utils".to_string(),
            },
            GraphEdge {
                source: "utils".to_string(),
                target: "base".to_string(),
            },
        ];

        let upstream = get_upstream_nodes(&["main".to_string()], &edges, None);

        assert!(upstream.contains("main"));
        assert!(upstream.contains("utils"));
        assert!(upstream.contains("base"));
    }

    #[test]
    fn test_downstream_nodes() {
        let edges = vec![
            GraphEdge {
                source: "main".to_string(),
                target: "utils".to_string(),
            },
            GraphEdge {
                source: "app".to_string(),
                target: "utils".to_string(),
            },
        ];

        let downstream = get_downstream_nodes(&["utils".to_string()], &edges, None);

        assert!(downstream.contains("utils"));
        assert!(downstream.contains("main"));
        assert!(downstream.contains("app"));
    }
}

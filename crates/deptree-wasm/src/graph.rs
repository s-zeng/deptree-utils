use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::{GraphEdge, GraphNode};

/// Compute BFS distances from a single node to all reachable nodes
pub fn bfs_distances_from_node(
    graph: &Graph<String, ()>,
    root_idx: NodeIndex,
) -> HashMap<String, usize> {
    let mut distances = HashMap::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back((root_idx, 0));
    visited.insert(root_idx);

    while let Some((node_idx, dist)) = queue.pop_front() {
        let node_id = &graph[node_idx];
        distances.insert(node_id.clone(), dist);

        for neighbor in graph.neighbors(node_idx) {
            if visited.insert(neighbor) {
                queue.push_back((neighbor, dist + 1));
            }
        }
    }

    distances
}

/// Compute distances from all nodes to all reachable nodes
/// Returns a map: node_id -> (reachable_node_id -> distance)
pub fn compute_all_distances(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> HashMap<String, HashMap<String, usize>> {
    // Build petgraph Graph
    let mut graph = Graph::<String, ()>::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

    // Add nodes
    for node in nodes {
        let idx = graph.add_node(node.id.clone());
        node_map.insert(node.id.clone(), idx);
    }

    // Add edges
    for edge in edges {
        if let (Some(&source_idx), Some(&target_idx)) =
            (node_map.get(&edge.source), node_map.get(&edge.target))
        {
            graph.add_edge(source_idx, target_idx, ());
        }
    }

    // Compute distances from each node
    let mut all_distances = HashMap::new();

    for node in nodes {
        if let Some(&root_idx) = node_map.get(&node.id) {
            let distances = bfs_distances_from_node(&graph, root_idx);
            all_distances.insert(node.id.clone(), distances);
        }
    }

    all_distances
}

/// Check if a node is an orphan (has no incoming or outgoing edges)
pub fn is_orphan_node(node_id: &str, edges: &[GraphEdge]) -> bool {
    let has_incoming = edges.iter().any(|e| e.target == node_id);
    let has_outgoing = edges.iter().any(|e| e.source == node_id);
    !has_incoming && !has_outgoing
}

/// Get all nodes within max_distance from any of the root nodes
/// Uses the precomputed distance map
#[allow(dead_code)]
pub fn get_nodes_within_distance(
    roots: &[String],
    max_distance: usize,
    distance_map: &HashMap<String, HashMap<String, usize>>,
) -> HashSet<String> {
    let mut result = HashSet::new();

    for root in roots {
        // Add the root itself
        result.insert(root.clone());

        // Add all nodes reachable from this root within max_distance
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

/// Get all upstream dependencies (nodes that the roots depend on)
pub fn get_upstream_nodes(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
) -> HashSet<String> {
    let mut result: HashSet<String> = roots.iter().cloned().collect();
    let mut queue: VecDeque<(String, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    let mut visited: HashSet<String> = roots.iter().cloned().collect();

    while let Some((node_id, dist)) = queue.pop_front() {
        // Skip if we've exceeded max_distance
        if let Some(max_dist) = max_distance {
            if dist >= max_dist {
                continue;
            }
        }

        // Find all nodes that this node depends on (outgoing edges from this node)
        for edge in edges {
            if edge.source == node_id && visited.insert(edge.target.clone()) {
                result.insert(edge.target.clone());
                queue.push_back((edge.target.clone(), dist + 1));
            }
        }
    }

    result
}

/// Get all downstream dependents (nodes that depend on the roots)
pub fn get_downstream_nodes(
    roots: &[String],
    edges: &[GraphEdge],
    max_distance: Option<usize>,
) -> HashSet<String> {
    let mut result: HashSet<String> = roots.iter().cloned().collect();
    let mut queue: VecDeque<(String, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    let mut visited: HashSet<String> = roots.iter().cloned().collect();

    while let Some((node_id, dist)) = queue.pop_front() {
        // Skip if we've exceeded max_distance
        if let Some(max_dist) = max_distance {
            if dist >= max_dist {
                continue;
            }
        }

        // Find all nodes that depend on this node (incoming edges to this node)
        for edge in edges {
            if edge.target == node_id && visited.insert(edge.source.clone()) {
                result.insert(edge.source.clone());
                queue.push_back((edge.source.clone(), dist + 1));
            }
        }
    }

    result
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

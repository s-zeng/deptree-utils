mod filters;
mod graph;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use wasm_bindgen::prelude::*;

/// Graph node representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String, // "module", "script", or "namespace"
    pub is_orphan: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlighted: Option<bool>,
}

/// Graph edge representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// Complete graph data
#[derive(Debug, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<GraphConfig>,
}

/// Configuration for graph visualization
#[derive(Debug, Serialize, Deserialize)]
pub struct GraphConfig {
    pub include_orphans: bool,
    pub include_namespaces: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlighted_modules: Option<Vec<String>>,
}

/// Filter configuration from JavaScript
#[derive(Debug, Deserialize)]
pub struct FilterConfig {
    #[serde(rename = "showOrphans")]
    pub show_orphans: bool,
    #[serde(rename = "showNamespaces")]
    pub show_namespaces: bool,
    #[serde(rename = "excludePatterns")]
    pub exclude_patterns: Vec<String>,
    #[serde(rename = "upstreamRoots")]
    pub upstream_roots: Vec<String>,
    #[serde(rename = "downstreamRoots")]
    pub downstream_roots: Vec<String>,
    #[serde(rename = "maxDistance")]
    pub max_distance: Option<usize>,
    #[serde(rename = "highlightedOnly")]
    pub highlighted_only: bool,
}

/// Result of filter operation containing both visibility and highlighting information
#[derive(Debug, Serialize, Deserialize)]
pub struct FilterResult {
    /// Node IDs that should be visible
    pub visible: Vec<String>,
    /// Node IDs that should be highlighted
    pub highlighted: Vec<String>,
}

/// Main graph processor exposed to JavaScript
#[wasm_bindgen]
pub struct GraphProcessor {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[wasm_bindgen]
impl GraphProcessor {
    /// Create a new GraphProcessor from JSON
    #[wasm_bindgen(constructor)]
    pub fn new(graph_json: &str) -> Result<GraphProcessor, JsValue> {
        let graph_data: GraphData = serde_json::from_str(graph_json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse graph JSON: {}", e)))?;

        Ok(GraphProcessor {
            nodes: graph_data.nodes,
            edges: graph_data.edges,
        })
    }

    /// Compute all-pairs shortest paths using BFS
    /// Returns JSON object with distances: { "node1": { "node2": 2, "node3": 1 }, ... }
    pub fn compute_all_distances(&self) -> JsValue {
        let distances = graph::compute_all_distances(&self.nodes, &self.edges);
        serde_wasm_bindgen::to_value(&distances).unwrap_or_else(|_| JsValue::NULL)
    }

    /// Check if a node is an orphan (no incoming or outgoing edges)
    pub fn is_orphan(&self, node_id: &str) -> bool {
        graph::is_orphan_node(node_id, &self.edges)
    }

    /// Filter nodes based on criteria
    /// Returns JSON object with both visible and highlighted node IDs
    pub fn filter_nodes(&self, filter_config_json: &str) -> JsValue {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"WASM filter_nodes called".into());

        let filter_config: FilterConfig = match serde_json::from_str(filter_config_json) {
            Ok(config) => config,
            Err(_e) => {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::error_1(&format!("Failed to parse filter config: {}", _e).into());
                let empty_result = FilterResult {
                    visible: Vec::new(),
                    highlighted: Vec::new(),
                };
                return serde_wasm_bindgen::to_value(&empty_result).unwrap();
            }
        };

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!(
                "Config parsed: highlightedOnly={}",
                filter_config.highlighted_only
            )
            .into(),
        );

        // Step 1: Compute filtered_set from upstream/downstream/distance filters
        let mut filtered_set: Option<HashSet<String>> = None;

        // Apply upstream filtering
        if !filter_config.upstream_roots.is_empty() {
            let upstream = graph::get_upstream_nodes(
                &filter_config.upstream_roots,
                &self.edges,
                filter_config.max_distance,
            );
            filtered_set = Some(upstream);
        }

        // Apply downstream filtering
        if !filter_config.downstream_roots.is_empty() {
            let downstream = graph::get_downstream_nodes(
                &filter_config.downstream_roots,
                &self.edges,
                filter_config.max_distance,
            );

            // If we already have upstream filter, intersect; otherwise just use downstream
            filtered_set = Some(match filtered_set {
                Some(upstream) => upstream.intersection(&downstream).cloned().collect(),
                None => downstream,
            });
        }

        // Step 2: Determine visible set based on highlightedOnly
        let visible_base = if filter_config.highlighted_only {
            if filtered_set.is_some() {
                // Interactive filters are active - show only filtered nodes
                filtered_set.clone()
            } else {
                // No interactive filters - check for CLI highlighting
                let cli_highlighted: HashSet<String> = self
                    .nodes
                    .iter()
                    .filter(|n| n.highlighted.unwrap_or(false))
                    .map(|n| n.id.clone())
                    .collect();

                if cli_highlighted.is_empty() {
                    // No CLI highlighting either - show all nodes (default state)
                    None
                } else {
                    // Show only CLI-highlighted nodes
                    Some(cli_highlighted)
                }
            }
        } else {
            // Show all nodes (highlightedOnly is false)
            None
        };

        // Step 3: Apply remaining filters (orphans, namespaces, patterns) to visible set
        let visible = filters::apply_filters(
            &self.nodes,
            filter_config.show_orphans,
            filter_config.show_namespaces,
            &filter_config.exclude_patterns,
            visible_base.as_ref(),
        );

        // Step 4: Determine highlighted set based on filter state
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&format!(
            "Filter state: filtered_set={}, upstream_roots={:?}, downstream_roots={:?}, show_orphans={}, show_namespaces={}, exclude_patterns={}, visible_count={}",
            filtered_set.is_some(),
            filter_config.upstream_roots,
            filter_config.downstream_roots,
            filter_config.show_orphans,
            filter_config.show_namespaces,
            filter_config.exclude_patterns.len(),
            visible.len()
        ).into());

        let highlighted_nodes: Vec<String> = if filtered_set.is_some() {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::log_1(&"Using upstream/downstream highlighting".into());

            // Upstream/downstream filters active - highlight those filtered nodes (but only if they're visible)
            let filter_set = filtered_set.as_ref().unwrap();
            visible
                .iter()
                .filter(|node_id| filter_set.contains(*node_id))
                .cloned()
                .collect()
        } else if !filter_config.show_orphans
            || !filter_config.show_namespaces
            || !filter_config.exclude_patterns.is_empty()
        {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::log_1(&"Using orphan/namespace/pattern highlighting".into());

            // Other interactive filters (orphans/namespaces/patterns) active - highlight visible nodes
            visible.iter().cloned().collect()
        } else {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::log_1(&"Using CLI highlighting".into());

            // No interactive filters - use CLI highlighting for backward compatibility
            self.nodes
                .iter()
                .filter(|n| n.highlighted.unwrap_or(false))
                .map(|n| n.id.clone())
                .collect()
        };

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!(
                "Highlighted {} out of {} visible nodes",
                highlighted_nodes.len(),
                visible.len()
            )
            .into(),
        );

        // Step 6: Return both visible and highlighted sets
        let result = FilterResult {
            visible: visible.into_iter().collect(),
            highlighted: highlighted_nodes,
        };

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!(
                "filter_nodes result: visible={}, highlighted={}",
                result.visible.len(),
                result.highlighted.len()
            )
            .into(),
        );

        serde_wasm_bindgen::to_value(&result).unwrap_or_else(|_| JsValue::NULL)
    }

    /// Get all upstream dependencies from given roots
    /// Returns JSON array of node IDs
    pub fn get_upstream(&self, roots: Vec<String>, max_distance: Option<usize>) -> JsValue {
        let upstream = graph::get_upstream_nodes(&roots, &self.edges, max_distance);
        let result: Vec<String> = upstream.into_iter().collect();
        serde_wasm_bindgen::to_value(&result).unwrap_or_else(|_| JsValue::NULL)
    }

    /// Get all downstream dependents from given roots
    /// Returns JSON array of node IDs
    pub fn get_downstream(&self, roots: Vec<String>, max_distance: Option<usize>) -> JsValue {
        let downstream = graph::get_downstream_nodes(&roots, &self.edges, max_distance);
        let result: Vec<String> = downstream.into_iter().collect();
        serde_wasm_bindgen::to_value(&result).unwrap_or_else(|_| JsValue::NULL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_processor_creation() {
        let graph_json = r#"{
            "nodes": [
                {"id": "module_a", "type": "module", "is_orphan": false},
                {"id": "module_b", "type": "module", "is_orphan": true}
            ],
            "edges": []
        }"#;

        let processor = GraphProcessor::new(graph_json);
        assert!(processor.is_ok());
    }

    #[test]
    fn test_is_orphan() {
        let graph_json = r#"{
            "nodes": [
                {"id": "module_a", "type": "module", "is_orphan": false},
                {"id": "module_b", "type": "module", "is_orphan": true}
            ],
            "edges": [
                {"source": "module_a", "target": "module_b"}
            ]
        }"#;

        let processor = GraphProcessor::new(graph_json).unwrap();
        assert!(!processor.is_orphan("module_a"));
        assert!(!processor.is_orphan("module_b")); // has incoming edge
    }

    #[test]
    fn test_compute_all_distances() {
        let graph_json = r#"{
            "nodes": [
                {"id": "a", "type": "module", "is_orphan": false},
                {"id": "b", "type": "module", "is_orphan": false},
                {"id": "c", "type": "module", "is_orphan": false}
            ],
            "edges": [
                {"source": "a", "target": "b"},
                {"source": "b", "target": "c"}
            ]
        }"#;

        let processor = GraphProcessor::new(graph_json).unwrap();

        // This test can only run on wasm32 target
        #[cfg(target_arch = "wasm32")]
        {
            let distances = processor.compute_all_distances();
            assert!(!distances.is_null());
        }

        // On non-wasm targets, just verify we can create the processor
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Test the underlying graph algorithm directly
            let distances = graph::compute_all_distances(&processor.nodes, &processor.edges);
            assert!(distances.contains_key("a"));
            assert_eq!(distances.get("a").and_then(|d| d.get("b")), Some(&1));
            assert_eq!(distances.get("a").and_then(|d| d.get("c")), Some(&2));
        }
    }

    // Tests for filter_nodes functionality
    #[cfg(test)]
    mod filter_nodes_tests {
        use super::*;
        use std::collections::HashSet;

        fn create_test_graph() -> (Vec<GraphNode>, Vec<GraphEdge>) {
            let nodes = vec![
                GraphNode {
                    id: "module_a".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "module_b".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "orphan_c".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: true,
                    highlighted: None,
                },
            ];

            let edges = vec![GraphEdge {
                source: "module_a".to_string(),
                target: "module_b".to_string(),
            }];

            (nodes, edges)
        }

        #[test]
        fn test_highlighted_only_no_filters_no_cli_highlighting() {
            let (nodes, edges) = create_test_graph();
            let processor = GraphProcessor { nodes, edges };

            // Apply filters directly using internal logic
            let filter_config = FilterConfig {
                show_orphans: true,
                show_namespaces: true,
                exclude_patterns: vec![],
                upstream_roots: vec![],
                downstream_roots: vec![],
                max_distance: None,
                highlighted_only: true,
            };

            // Simulate the logic from filter_nodes
            let filtered_set: Option<HashSet<String>> = None; // No upstream/downstream filters

            // Determine visible_base (this is where the bug should be)
            let cli_highlighted: HashSet<String> = processor
                .nodes
                .iter()
                .filter(|n| n.highlighted.unwrap_or(false))
                .map(|n| n.id.clone())
                .collect();

            let visible_base = if filter_config.highlighted_only {
                if filtered_set.is_some() {
                    filtered_set.clone()
                } else if cli_highlighted.is_empty() {
                    // BUG LOCATION: When no CLI highlighting, should show all nodes
                    None
                } else {
                    Some(cli_highlighted.clone())
                }
            } else {
                None
            };

            // Apply remaining filters
            let visible = filters::apply_filters(
                &processor.nodes,
                filter_config.show_orphans,
                filter_config.show_namespaces,
                &filter_config.exclude_patterns,
                visible_base.as_ref(),
            );

            // All nodes should be visible (default state)
            assert_eq!(
                visible.len(),
                3,
                "Expected all 3 nodes to be visible, got {}",
                visible.len()
            );
            assert!(visible.contains("module_a"), "module_a should be visible");
            assert!(visible.contains("module_b"), "module_b should be visible");
            assert!(visible.contains("orphan_c"), "orphan_c should be visible");
        }

        #[test]
        fn test_orphan_filter_highlights_visible_nodes() {
            let (nodes, edges) = create_test_graph();
            let graph_data = GraphData {
                nodes,
                edges,
                config: None,
            };
            let graph_json = serde_json::to_string(&graph_data).unwrap();
            let processor = GraphProcessor::new(&graph_json).unwrap();

            let filter_config_json = r#"{
                "showOrphans": false,
                "showNamespaces": true,
                "excludePatterns": [],
                "upstreamRoots": [],
                "downstreamRoots": [],
                "maxDistance": null,
                "highlightedOnly": false
            }"#;

            #[cfg(target_arch = "wasm32")]
            {
                let result_js = processor.filter_nodes(filter_config_json);
                let result: FilterResult = serde_wasm_bindgen::from_value(result_js).unwrap();

                // Should have 2 visible nodes (non-orphans)
                assert_eq!(result.visible.len(), 2);
                assert!(result.visible.contains(&"module_a".to_string()));
                assert!(result.visible.contains(&"module_b".to_string()));

                // All visible nodes should be highlighted
                assert_eq!(result.highlighted.len(), 2);
                assert!(result.highlighted.contains(&"module_a".to_string()));
                assert!(result.highlighted.contains(&"module_b".to_string()));
            }
        }

        #[test]
        fn test_namespace_filter_highlights_visible_nodes() {
            let nodes = vec![
                GraphNode {
                    id: "module_a".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "module_b".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "namespace_pkg".to_string(),
                    node_type: "namespace".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
            ];
            let edges = vec![GraphEdge {
                source: "module_a".to_string(),
                target: "module_b".to_string(),
            }];

            let graph_data = GraphData {
                nodes,
                edges,
                config: None,
            };
            let graph_json = serde_json::to_string(&graph_data).unwrap();
            let processor = GraphProcessor::new(&graph_json).unwrap();

            let filter_config_json = r#"{
                "showOrphans": true,
                "showNamespaces": false,
                "excludePatterns": [],
                "upstreamRoots": [],
                "downstreamRoots": [],
                "maxDistance": null,
                "highlightedOnly": false
            }"#;

            #[cfg(target_arch = "wasm32")]
            {
                let result_js = processor.filter_nodes(filter_config_json);
                let result: FilterResult = serde_wasm_bindgen::from_value(result_js).unwrap();

                // Should have 2 visible nodes (non-namespaces)
                assert_eq!(result.visible.len(), 2);
                assert!(result.visible.contains(&"module_a".to_string()));
                assert!(result.visible.contains(&"module_b".to_string()));

                // All visible nodes should be highlighted
                assert_eq!(result.highlighted.len(), 2);
                assert!(result.highlighted.contains(&"module_a".to_string()));
                assert!(result.highlighted.contains(&"module_b".to_string()));
            }
        }

        #[test]
        fn test_script_exclusion_highlights_visible_nodes() {
            let nodes = vec![
                GraphNode {
                    id: "scripts.main".to_string(),
                    node_type: "script".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "scripts.old_runner".to_string(),
                    node_type: "script".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
            ];
            let edges = vec![];

            let graph_data = GraphData {
                nodes,
                edges,
                config: None,
            };
            let graph_json = serde_json::to_string(&graph_data).unwrap();
            let processor = GraphProcessor::new(&graph_json).unwrap();

            let filter_config_json = r#"{
                "showOrphans": true,
                "showNamespaces": true,
                "excludePatterns": ["*old*"],
                "upstreamRoots": [],
                "downstreamRoots": [],
                "maxDistance": null,
                "highlightedOnly": false
            }"#;

            #[cfg(target_arch = "wasm32")]
            {
                let result_js = processor.filter_nodes(filter_config_json);
                let result: FilterResult = serde_wasm_bindgen::from_value(result_js).unwrap();

                // Should have 1 visible node (scripts.main)
                assert_eq!(result.visible.len(), 1);
                assert!(result.visible.contains(&"scripts.main".to_string()));

                // Visible node should be highlighted
                assert_eq!(result.highlighted.len(), 1);
                assert!(result.highlighted.contains(&"scripts.main".to_string()));
            }
        }

        #[test]
        fn test_cli_highlighting_preserved_when_no_interactive_filters() {
            let nodes = vec![
                GraphNode {
                    id: "module_a".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: Some(true), // CLI-highlighted
                },
                GraphNode {
                    id: "module_b".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: Some(true), // CLI-highlighted
                },
                GraphNode {
                    id: "module_c".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
            ];
            let edges = vec![];

            let graph_data = GraphData {
                nodes,
                edges,
                config: None,
            };
            let graph_json = serde_json::to_string(&graph_data).unwrap();
            let processor = GraphProcessor::new(&graph_json).unwrap();

            let filter_config_json = r#"{
                "showOrphans": true,
                "showNamespaces": true,
                "excludePatterns": [],
                "upstreamRoots": [],
                "downstreamRoots": [],
                "maxDistance": null,
                "highlightedOnly": false
            }"#;

            #[cfg(target_arch = "wasm32")]
            {
                let result_js = processor.filter_nodes(filter_config_json);
                let result: FilterResult = serde_wasm_bindgen::from_value(result_js).unwrap();

                // All 3 nodes should be visible
                assert_eq!(result.visible.len(), 3);

                // Only CLI-highlighted nodes should be highlighted
                assert_eq!(result.highlighted.len(), 2);
                assert!(result.highlighted.contains(&"module_a".to_string()));
                assert!(result.highlighted.contains(&"module_b".to_string()));
                assert!(!result.highlighted.contains(&"module_c".to_string()));
            }
        }

        #[test]
        fn test_combined_filters_highlight_intersection() {
            let nodes = vec![
                GraphNode {
                    id: "module_a".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "module_b".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: false,
                    highlighted: None,
                },
                GraphNode {
                    id: "orphan_c".to_string(),
                    node_type: "module".to_string(),
                    is_orphan: true,
                    highlighted: None,
                },
            ];
            let edges = vec![
                GraphEdge {
                    source: "module_a".to_string(),
                    target: "module_b".to_string(),
                },
                GraphEdge {
                    source: "module_a".to_string(),
                    target: "orphan_c".to_string(),
                },
            ];

            let graph_data = GraphData {
                nodes,
                edges,
                config: None,
            };
            let graph_json = serde_json::to_string(&graph_data).unwrap();
            let processor = GraphProcessor::new(&graph_json).unwrap();

            let filter_config_json = r#"{
                "showOrphans": false,
                "showNamespaces": true,
                "excludePatterns": [],
                "upstreamRoots": ["module_b"],
                "downstreamRoots": [],
                "maxDistance": null,
                "highlightedOnly": false
            }"#;

            #[cfg(target_arch = "wasm32")]
            {
                let result_js = processor.filter_nodes(filter_config_json);
                let result: FilterResult = serde_wasm_bindgen::from_value(result_js).unwrap();

                // Should show upstream of module_b (module_a, module_b) excluding orphans
                assert_eq!(result.visible.len(), 2);
                assert!(result.visible.contains(&"module_a".to_string()));
                assert!(result.visible.contains(&"module_b".to_string()));

                // All visible nodes should be highlighted
                assert_eq!(result.highlighted.len(), 2);
                assert!(result.highlighted.contains(&"module_a".to_string()));
                assert!(result.highlighted.contains(&"module_b".to_string()));
            }
        }

        #[test]
        fn test_highlighted_only_with_interactive_filters() {
            let (nodes, edges) = create_test_graph();
            let graph_data = GraphData {
                nodes,
                edges,
                config: None,
            };
            let graph_json = serde_json::to_string(&graph_data).unwrap();
            let processor = GraphProcessor::new(&graph_json).unwrap();

            let filter_config_json = r#"{
                "showOrphans": false,
                "showNamespaces": true,
                "excludePatterns": [],
                "upstreamRoots": [],
                "downstreamRoots": [],
                "maxDistance": null,
                "highlightedOnly": true
            }"#;

            #[cfg(target_arch = "wasm32")]
            {
                let result_js = processor.filter_nodes(filter_config_json);
                let result: FilterResult = serde_wasm_bindgen::from_value(result_js).unwrap();

                // Should show only non-orphan nodes
                assert_eq!(result.visible.len(), 2);
                assert!(result.visible.contains(&"module_a".to_string()));
                assert!(result.visible.contains(&"module_b".to_string()));

                // All visible nodes should be highlighted
                assert_eq!(result.highlighted.len(), 2);
                assert!(result.highlighted.contains(&"module_a".to_string()));
                assert!(result.highlighted.contains(&"module_b".to_string()));
            }
        }
    }
}

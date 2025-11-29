use std::collections::HashSet;

use crate::GraphNode;

/// Match a string against a wildcard pattern.
/// Supports: *prefix, suffix*, *substring*.
pub fn matches_pattern(text: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');

    match (starts_with_wildcard, ends_with_wildcard) {
        (true, true) => {
            // *substring*
            let substring = &pattern[1..pattern.len() - 1];
            text.contains(substring)
        }
        (true, false) => {
            // *suffix
            let suffix = &pattern[1..];
            text.ends_with(suffix)
        }
        (false, true) => {
            // prefix*
            let prefix = &pattern[..pattern.len() - 1];
            text.starts_with(prefix)
        }
        (false, false) => {
            // exact match (or substring match for backwards compatibility)
            text.contains(pattern)
        }
    }
}

/// Filter nodes based on multiple criteria.
pub fn apply_filters(
    nodes: &[GraphNode],
    show_orphans: bool,
    show_namespaces: bool,
    exclude_patterns: &[String],
    filtered_set: Option<&HashSet<String>>, // If Some, only include nodes in this set
) -> HashSet<String> {
    nodes
        .iter()
        .filter(|node| {
            filtered_set
                .map(|set| set.contains(&node.id))
                .unwrap_or(true)
        })
        .filter(|node| show_orphans || !node.is_orphan)
        .filter(|node| show_namespaces || node.node_type != "namespace")
        .filter(|node| {
            node.node_type != "script"
                || !exclude_patterns
                    .iter()
                    .any(|pattern| matches_pattern(&node.id, pattern))
        })
        .map(|node| node.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("test_script.py", "*test*"));
        assert!(matches_pattern("test_script.py", "test*"));
        assert!(matches_pattern("test_script.py", "*.py"));
        assert!(matches_pattern("test_script.py", "script"));

        assert!(!matches_pattern("test_script.py", "*foo*"));
        assert!(!matches_pattern("test_script.py", "foo*"));
    }

    #[test]
    fn test_apply_filters_orphans() {
        let nodes = vec![
            GraphNode {
                id: "module_a".to_string(),
                node_type: "module".to_string(),
                is_orphan: false,
                highlighted: None,
                parent: None,
            },
            GraphNode {
                id: "orphan".to_string(),
                node_type: "module".to_string(),
                is_orphan: true,
                highlighted: None,
                parent: None,
            },
        ];

        let visible = apply_filters(&nodes, false, true, &[], None);
        assert!(visible.contains("module_a"));
        assert!(!visible.contains("orphan"));

        let visible = apply_filters(&nodes, true, true, &[], None);
        assert!(visible.contains("module_a"));
        assert!(visible.contains("orphan"));
    }

    #[test]
    fn test_apply_filters_namespaces() {
        let nodes = vec![
            GraphNode {
                id: "module_a".to_string(),
                node_type: "module".to_string(),
                is_orphan: false,
                highlighted: None,
                parent: None,
            },
            GraphNode {
                id: "namespace_pkg".to_string(),
                node_type: "namespace".to_string(),
                is_orphan: false,
                highlighted: None,
                parent: None,
            },
        ];

        let visible = apply_filters(&nodes, true, false, &[], None);
        assert!(visible.contains("module_a"));
        assert!(!visible.contains("namespace_pkg"));

        let visible = apply_filters(&nodes, true, true, &[], None);
        assert!(visible.contains("module_a"));
        assert!(visible.contains("namespace_pkg"));
    }

    #[test]
    fn test_apply_filters_exclude_patterns() {
        let nodes = vec![
            GraphNode {
                id: "scripts.main".to_string(),
                node_type: "script".to_string(),
                is_orphan: false,
                highlighted: None,
                parent: None,
            },
            GraphNode {
                id: "scripts.old_runner".to_string(),
                node_type: "script".to_string(),
                is_orphan: false,
                highlighted: None,
                parent: None,
            },
        ];

        let patterns = vec!["*old*".to_string()];
        let visible = apply_filters(&nodes, true, true, &patterns, None);

        assert!(visible.contains("scripts.main"));
        assert!(!visible.contains("scripts.old_runner"));
    }
}

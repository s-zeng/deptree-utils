import type { GraphData, GraphNode, GraphEdge } from "../../src/types";

export const sampleNodes: GraphNode[] = [
  { id: "pkg.module_a", type: "module", is_orphan: false },
  { id: "pkg.module_b", type: "module", is_orphan: false },
  { id: "scripts.runner", type: "script", is_orphan: false },
  { id: "orphan", type: "module", is_orphan: true },
];

export const sampleEdges: GraphEdge[] = [
  { source: "pkg.module_a", target: "pkg.module_b" },
  { source: "scripts.runner", target: "pkg.module_a" },
];

export const sampleGraphData: GraphData = {
  nodes: sampleNodes,
  edges: sampleEdges,
  config: {
    include_orphans: false,
    include_namespaces: false,
  },
};

// Test data for compound nodes (namespace groups)
export const compoundNodes: GraphNode[] = [
  // Namespace group (parent node)
  { id: "pkg.foo", type: "namespace_group", is_orphan: false },

  // Child modules under pkg.foo
  {
    id: "pkg.foo.module_a",
    type: "module",
    is_orphan: false,
    parent: "pkg.foo",
  },
  {
    id: "pkg.foo.module_b",
    type: "module",
    is_orphan: false,
    parent: "pkg.foo",
  },

  // Another namespace group
  { id: "pkg.bar", type: "namespace_group", is_orphan: false },

  // Child module under pkg.bar
  {
    id: "pkg.bar.module_c",
    type: "module",
    is_orphan: false,
    parent: "pkg.bar",
  },

  // Standalone module (no parent)
  { id: "pkg.standalone", type: "module", is_orphan: false },
];

export const compoundEdges: GraphEdge[] = [
  // Edge between nodes in same parent
  { source: "pkg.foo.module_a", target: "pkg.foo.module_b" },

  // Edge between nodes in different parents (crosses hierarchy)
  { source: "pkg.foo.module_b", target: "pkg.bar.module_c" },

  // Edge from child to standalone module
  { source: "pkg.bar.module_c", target: "pkg.standalone" },
];

export const compoundGraphData: GraphData = {
  nodes: compoundNodes,
  edges: compoundEdges,
  config: {
    include_orphans: true,
    include_namespaces: true,
  },
};

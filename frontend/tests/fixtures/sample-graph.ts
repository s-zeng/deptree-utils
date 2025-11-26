import type { GraphData, GraphNode, GraphEdge } from '../../src/types';

export const sampleNodes: GraphNode[] = [
  { id: 'pkg.module_a', type: 'module', is_orphan: false },
  { id: 'pkg.module_b', type: 'module', is_orphan: false },
  { id: 'scripts.runner', type: 'script', is_orphan: false },
  { id: 'orphan', type: 'module', is_orphan: true },
];

export const sampleEdges: GraphEdge[] = [
  { source: 'pkg.module_a', target: 'pkg.module_b' },
  { source: 'scripts.runner', target: 'pkg.module_a' },
];

export const sampleGraphData: GraphData = {
  nodes: sampleNodes,
  edges: sampleEdges,
  config: {
    include_orphans: false,
    include_namespaces: false,
  },
};

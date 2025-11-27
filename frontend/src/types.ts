// Type definitions for graph data

export interface GraphNode {
  id: string;
  type: 'module' | 'script' | 'namespace' | 'namespace_group';
  is_orphan: boolean;
  highlighted?: boolean;
  parent?: string;
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface GraphConfig {
  include_orphans: boolean;
  include_namespaces: boolean;
  highlighted_modules?: string[];
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  config?: GraphConfig;
}

export interface FilterConfig {
  showOrphans: boolean;
  showNamespaces: boolean;
  excludePatterns: string[];
  upstreamRoots: Set<string>;
  downstreamRoots: Set<string>;
  maxDistance: number | null;
  highlightedOnly: boolean;
}

export interface FilterResult {
  visible: string[];
  highlighted: string[];
}

export interface DistanceMap {
  [nodeId: string]: {
    [targetId: string]: number;
  };
}

// Global type augmentation for graph data injected by Rust
declare global {
  interface Window {
    __GRAPH_DATA__: GraphData;
  }
}

export {};

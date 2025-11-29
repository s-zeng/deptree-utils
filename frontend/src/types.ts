import type { GraphConfig } from "./bindings/GraphConfig";
import type { GraphData } from "./bindings/GraphData";
import type { GraphEdge } from "./bindings/GraphEdge";
import type { GraphNode } from "./bindings/GraphNode";

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

export type { GraphConfig, GraphData, GraphEdge, GraphNode };

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

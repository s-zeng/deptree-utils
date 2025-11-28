import type * as cytoscape from "cytoscape";

export type LayoutOptionsWithExtensions = cytoscape.LayoutOptions & {
  elk?: Record<string, unknown>;
  rankDir?: "TB" | "LR" | "BT" | "RL";
  nodeSep?: number;
  rankSep?: number;
  padding?: number;
  animate?: boolean;
  animationDuration?: number;
  nodeDimensionsIncludeLabels?: boolean;
  [key: string]: unknown;
};

export type CytoscapeNamespace = typeof import("cytoscape");

import type { GraphData, DistanceMap } from "./types";
import type cytoscape from "cytoscape";

// Declare Cytoscape as a global (loaded from CDN)
declare const cytoscape: typeof import("cytoscape");

export const HIGHLIGHT_SELECTOR = "node[?highlighted]";

/**
 * Initialize Cytoscape with graph data and styling
 */
export function initializeCytoscape(
  graphData: GraphData,
  distances: DistanceMap,
): cytoscape.Core {
  // Register layout extensions
  registerLayoutExtensions();

  // Transform graph data to Cytoscape elements format
  const elements = transformToElements(graphData, distances);

  // Initialize Cytoscape
  const cy = cytoscape({
    container: document.getElementById("cy"),

    elements,

    style: getCytoscapeStyles(),

    // Initial layout will be set by layout manager
    layout: {
      name: "dagre",
      rankDir: "LR",
      nodeSep: 50,
      rankSep: 100,
      padding: 30,
    },
  });

  // Setup event handlers
  setupEventHandlers(cy);

  return cy;
}

/**
 * Register Cytoscape layout extension libraries
 */
function registerLayoutExtensions(): void {
  if (typeof cytoscape === "undefined") {
    console.warn("Cytoscape not loaded");
    return;
  }

  // Register cose-bilkent
  if (typeof (window as any).cytoscapeCoseBilkent !== "undefined") {
    cytoscape.use((window as any).cytoscapeCoseBilkent);
  }

  // Register cola
  if (typeof (window as any).cytoscapeCola !== "undefined") {
    cytoscape.use((window as any).cytoscapeCola);
  }

  // Register elk
  if (typeof (window as any).cytoscapeElk !== "undefined") {
    cytoscape.use((window as any).cytoscapeElk);
  }

  // Register dagre
  if (typeof (window as any).cytoscapeDagre !== "undefined") {
    cytoscape.use((window as any).cytoscapeDagre);
  }
}

/**
 * Transform graph data to Cytoscape elements format
 */
function transformToElements(
  graphData: GraphData,
  distances: DistanceMap,
): cytoscape.ElementDefinition[] {
  const elements: cytoscape.ElementDefinition[] = [];

  // Add nodes
  for (const node of graphData.nodes) {
    const data: Record<string, any> = {
      id: node.id,
      label: node.id,
      type: node.type,
      is_orphan: node.is_orphan,
      // Store distance data for filtering
      all_distances: distances[node.id] || {},
    };

    // Set parent for compound nodes
    if (node.parent) {
      data.parent = node.parent;
    }

    // Only set highlighted attribute if true (so CSS selector won't match false values)
    if (node.highlighted) {
      data.highlighted = true;
    }

    elements.push({ data });
  }

  // Add edges
  for (const edge of graphData.edges) {
    elements.push({
      data: {
        source: edge.source,
        target: edge.target,
      },
    });
  }

  return elements;
}

/**
 * Get Cytoscape style definitions
 */
export function getCytoscapeStyles(): cytoscape.Stylesheet[] {
  return [
    // Default node style
    {
      selector: "node",
      style: {
        label: "data(label)",
        "text-valign": "center",
        "text-halign": "center",
        "font-size": "12px",
        "background-color": "#90caf9",
        "border-width": 1,
        "border-color": "#1976d2",
        width: "label",
        height: "label",
        padding: "10px",
        shape: "ellipse",
        "text-wrap": "wrap",
        "text-max-width": "150px",
      },
    },

    // Script nodes (rectangular shape, green)
    {
      selector: 'node[type="script"]',
      style: {
        shape: "rectangle",
        "background-color": "#a5d6a7",
        "border-color": "#388e3c",
      },
    },

    // Namespace package nodes (hexagon, orange, dashed)
    {
      selector: 'node[type="namespace"]',
      style: {
        shape: "hexagon",
        "background-color": "#ffcc80",
        "border-color": "#f57c00",
        "border-style": "dashed",
      },
    },

    // Highlighted nodes (filtered results)
    {
      // Use truthy check so nodes with highlighted=false won't be styled
      selector: HIGHLIGHT_SELECTOR,
      style: {
        "background-color": "#ffeb3b", // Bright yellow
        "border-width": 4,
        "border-color": "#f57f17", // Dark amber/orange border
      },
    },

    // Parent nodes (namespace groups) - must use rectangle shape for compound nodes
    {
      selector: "node:parent",
      style: {
        "background-color": "#e3f2fd",
        "background-opacity": 0.3,
        "border-width": 2,
        "border-color": "#1976d2",
        "border-style": "dashed",
        shape: "rectangle",
        label: "data(label)",
        "text-valign": "top",
        "text-halign": "center",
        "font-size": "14px",
        "font-weight": "bold",
        padding: "20px",
      },
    },

    // Namespace group type (pure parent nodes)
    {
      selector: 'node[type="namespace_group"]',
      style: {
        "background-color": "#fff3e0",
        "background-opacity": 0.2,
        "border-color": "#ff9800",
      },
    },

    // Edges
    {
      selector: "edge",
      style: {
        width: 2,
        "line-color": "#999",
        "target-arrow-color": "#999",
        "target-arrow-shape": "triangle",
        "curve-style": "bezier",
        "arrow-scale": 1.2,
      },
    },
  ];
}

/**
 * Setup Cytoscape event handlers
 */
function setupEventHandlers(cy: cytoscape.Core): void {
  // Update info panel on node selection
  cy.on("select", "node", (evt) => {
    const node = evt.target;
    const info = document.getElementById("info");
    if (info) {
      info.textContent = `Selected: ${node.data("label")}`;
    }
  });

  cy.on("unselect", "node", () => {
    const info = document.getElementById("info");
    if (info) {
      info.textContent = "";
    }
  });
}

/**
 * Control functions for Cytoscape
 */
export const cytoscapeControls = {
  fitGraph(cy: cytoscape.Core): void {
    cy.fit(undefined, 50);
  },

  resetZoom(cy: cytoscape.Core): void {
    cy.zoom(1);
    cy.center();
  },

  centerGraph(cy: cytoscape.Core): void {
    cy.center();
  },

  exportPNG(cy: cytoscape.Core): void {
    const png = cy.png({ full: true, scale: 2 });
    const link = document.createElement("a");
    link.download = "dependency-graph.png";
    link.href = png;
    link.click();
  },
};

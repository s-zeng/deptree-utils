// Layout configuration definitions for all 9 supported layouts

export interface LayoutSetting {
  label: string;
  type: "select" | "number" | "checkbox";
  options?: { value: string | number; label: string }[];
  min?: number;
  max?: number;
  default: any;
  step?: number;
  nullable?: boolean;
}

export interface LayoutConfig {
  name: string;
  description: string;
  category: "built-in" | "extension";
  settings: {
    key: Record<string, LayoutSetting>;
    advanced: Record<string, LayoutSetting>;
  };
}

export const LAYOUT_CONFIGS: Record<string, LayoutConfig> = {
  dagre: {
    name: "Dagre",
    description: "Hierarchical directed acyclic graph",
    category: "built-in",
    settings: {
      key: {
        rankDir: {
          label: "Direction",
          type: "select",
          options: [
            { value: "TB", label: "Top to Bottom" },
            { value: "LR", label: "Left to Right" },
            { value: "BT", label: "Bottom to Top" },
            { value: "RL", label: "Right to Left" },
          ],
          default: "LR",
        },
        nodeSep: {
          label: "Node Separation",
          type: "number",
          min: 10,
          max: 200,
          default: 50,
          step: 10,
        },
      },
      advanced: {
        rankSep: {
          label: "Rank Separation",
          type: "number",
          min: 20,
          max: 300,
          default: 100,
          step: 10,
        },
        padding: {
          label: "Padding",
          type: "number",
          min: 0,
          max: 100,
          default: 30,
          step: 5,
        },
      },
    },
  },

  cose: {
    name: "CoSE",
    description: "Force-directed spring embedder",
    category: "built-in",
    settings: {
      key: {
        nodeRepulsion: {
          label: "Node Repulsion",
          type: "number",
          min: 100,
          max: 10000,
          default: 400000,
          step: 1000,
        },
        idealEdgeLength: {
          label: "Edge Length",
          type: "number",
          min: 10,
          max: 500,
          default: 100,
          step: 10,
        },
      },
      advanced: {
        gravity: {
          label: "Gravity",
          type: "number",
          min: 0,
          max: 10,
          default: 1,
          step: 0.1,
        },
        numIter: {
          label: "Iterations",
          type: "number",
          min: 100,
          max: 5000,
          default: 1000,
          step: 100,
        },
      },
    },
  },

  breadthfirst: {
    name: "Breadthfirst",
    description: "Tree layout from roots",
    category: "built-in",
    settings: {
      key: {
        directed: {
          label: "Use Edge Direction",
          type: "checkbox",
          default: true,
        },
        circle: {
          label: "Circular Layout",
          type: "checkbox",
          default: false,
        },
      },
      advanced: {
        spacingFactor: {
          label: "Spacing Factor",
          type: "number",
          min: 0.5,
          max: 3,
          default: 1.75,
          step: 0.25,
        },
      },
    },
  },

  circle: {
    name: "Circle",
    description: "Nodes in a circle",
    category: "built-in",
    settings: {
      key: {
        radius: {
          label: "Radius (leave empty for auto)",
          type: "number",
          min: 50,
          max: 1000,
          default: null,
          step: 50,
          nullable: true,
        },
      },
      advanced: {
        startAngle: {
          label: "Start Angle (radians)",
          type: "number",
          min: 0,
          max: 6.28,
          default: 3.14159,
          step: 0.1,
        },
        sweep: {
          label: "Sweep (radians)",
          type: "number",
          min: 0.1,
          max: 6.28,
          default: 6.28318,
          step: 0.1,
        },
      },
    },
  },

  grid: {
    name: "Grid",
    description: "Regular grid arrangement",
    category: "built-in",
    settings: {
      key: {
        avoidOverlap: {
          label: "Avoid Overlap",
          type: "checkbox",
          default: true,
        },
      },
      advanced: {
        rows: {
          label: "Rows (leave empty for auto)",
          type: "number",
          min: 1,
          max: 50,
          default: null,
          step: 1,
          nullable: true,
        },
        cols: {
          label: "Columns (leave empty for auto)",
          type: "number",
          min: 1,
          max: 50,
          default: null,
          step: 1,
          nullable: true,
        },
      },
    },
  },

  concentric: {
    name: "Concentric",
    description: "Concentric circles by importance",
    category: "built-in",
    settings: {
      key: {
        minNodeSpacing: {
          label: "Min Node Spacing",
          type: "number",
          min: 10,
          max: 200,
          default: 50,
          step: 10,
        },
      },
      advanced: {
        startAngle: {
          label: "Start Angle (radians)",
          type: "number",
          min: 0,
          max: 6.28,
          default: 3.14159,
          step: 0.1,
        },
      },
    },
  },

  "cose-bilkent": {
    name: "CoSE-Bilkent",
    description: "Enhanced force-directed (better quality)",
    category: "extension",
    settings: {
      key: {
        nodeRepulsion: {
          label: "Node Repulsion",
          type: "number",
          min: 100,
          max: 10000,
          default: 4500,
          step: 100,
        },
        idealEdgeLength: {
          label: "Edge Length",
          type: "number",
          min: 10,
          max: 500,
          default: 100,
          step: 10,
        },
      },
      advanced: {
        quality: {
          label: "Quality",
          type: "select",
          options: [
            { value: "default", label: "Default" },
            { value: "draft", label: "Draft (faster)" },
            { value: "proof", label: "Proof (better quality)" },
          ],
          default: "default",
        },
        gravity: {
          label: "Gravity",
          type: "number",
          min: 0,
          max: 1,
          default: 0.25,
          step: 0.05,
        },
      },
    },
  },

  cola: {
    name: "Cola",
    description: "Constraint-based force-directed",
    category: "extension",
    settings: {
      key: {
        edgeLength: {
          label: "Edge Length",
          type: "number",
          min: 10,
          max: 500,
          default: 100,
          step: 10,
        },
        nodeSpacing: {
          label: "Node Spacing",
          type: "number",
          min: 5,
          max: 100,
          default: 20,
          step: 5,
        },
      },
      advanced: {
        convergenceThreshold: {
          label: "Convergence Threshold",
          type: "number",
          min: 0.001,
          max: 0.1,
          default: 0.01,
          step: 0.001,
        },
        maxSimulationTime: {
          label: "Max Time (ms)",
          type: "number",
          min: 1000,
          max: 10000,
          default: 4000,
          step: 500,
        },
      },
    },
  },

  elk: {
    name: "ELK",
    description: "Eclipse Layout Kernel (advanced)",
    category: "extension",
    settings: {
      key: {
        algorithm: {
          label: "Algorithm",
          type: "select",
          options: [
            { value: "layered", label: "Layered (hierarchical)" },
            { value: "force", label: "Force" },
            { value: "stress", label: "Stress" },
            { value: "mrtree", label: "MR Tree" },
          ],
          default: "layered",
        },
        "elk.direction": {
          label: "Direction",
          type: "select",
          options: [
            { value: "DOWN", label: "Top to Bottom" },
            { value: "RIGHT", label: "Left to Right" },
            { value: "UP", label: "Bottom to Top" },
            { value: "LEFT", label: "Right to Left" },
          ],
          default: "RIGHT",
        },
      },
      advanced: {
        "elk.spacing.nodeNode": {
          label: "Node Spacing",
          type: "number",
          min: 10,
          max: 200,
          default: 80,
          step: 10,
        },
        "elk.layered.spacing.nodeNodeBetweenLayers": {
          label: "Layer Spacing",
          type: "number",
          min: 10,
          max: 200,
          default: 100,
          step: 10,
        },
        "elk.hierarchyHandling": {
          label: "Hierarchy Handling",
          type: "select",
          options: [
            {
              value: "INCLUDE_CHILDREN",
              label: "Include Children (recommended)",
            },
            { value: "SEPARATE_CHILDREN", label: "Separate Children" },
            { value: "INHERIT", label: "Inherit" },
          ],
          default: "INCLUDE_CHILDREN",
        },
      },
    },
  },
};

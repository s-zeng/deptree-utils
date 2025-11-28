import { afterEach, vi } from "vitest";

// Mock window.__GRAPH_DATA__ for tests
declare global {
  interface Window {
    __GRAPH_DATA__: any;
  }
}

Object.defineProperty(window, "__GRAPH_DATA__", {
  value: {
    nodes: [
      { id: "module_a", type: "module", is_orphan: false },
      { id: "module_b", type: "module", is_orphan: false },
    ],
    edges: [{ source: "module_a", target: "module_b" }],
    config: { include_orphans: false, include_namespaces: false },
  },
  writable: true,
});

// Mock WASM module - return native JS values, not JSON strings
vi.mock("../src/wasm/deptree_wasm", () => ({
  default: vi.fn(() => Promise.resolve()),
  GraphProcessor: vi.fn().mockImplementation((graphJson: string) => ({
    compute_all_distances: vi.fn(() => ({
      module_a: { module_b: 1 },
      module_b: {},
    })),
    filter_nodes: vi.fn((configJson: string) => {
      // Return native array, NOT JSON string
      return ["module_a", "module_b"];
    }),
    get_upstream: vi.fn((rootsJson: string) => {
      return JSON.parse(rootsJson); // Return native array
    }),
    get_downstream: vi.fn((rootsJson: string) => {
      return JSON.parse(rootsJson); // Return native array
    }),
  })),
}));

afterEach(() => {
  vi.clearAllMocks();
});

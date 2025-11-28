import { describe, it, expect, beforeEach, vi } from "vitest";
import { LayoutManager } from "./layout-manager";
import type cytoscape from "cytoscape";
import { compoundGraphData } from "../tests/fixtures/sample-graph";

// Mock Cytoscape instance with compound node support
function createMockCytoscapeWithCompoundNodes() {
  // Create mock nodes with parent-child relationships
  const mockNodes = compoundGraphData.nodes.map((node) => ({
    id: () => node.id,
    data: vi.fn((key?: string) => {
      if (key === "parent") return node.parent;
      if (key === "type") return node.type;
      return node.id;
    }),
    isParent: () => !!compoundGraphData.nodes.some((n) => n.parent === node.id),
    isChild: () => !!node.parent,
  }));

  // Create mock edges
  const mockEdges = compoundGraphData.edges.map((edge) => ({
    source: () => ({ id: () => edge.source }),
    target: () => ({ id: () => edge.target }),
  }));

  return {
    nodes: vi.fn(() => mockNodes),
    edges: vi.fn(() => mockEdges),
    layout: vi.fn((options: any) => ({
      run: vi.fn(),
      stop: vi.fn(),
    })),
  } as any;
}

describe("LayoutManager - ELK with Compound Nodes", () => {
  let layoutManager: LayoutManager;
  let mockCy: any;

  beforeEach(() => {
    mockCy = createMockCytoscapeWithCompoundNodes();
    layoutManager = new LayoutManager(mockCy);
  });

  describe("ELK layout configuration", () => {
    it("should include hierarchyHandling option by default", () => {
      layoutManager.setLayout("elk");
      const options = layoutManager.getLayoutOptions();

      expect(options.name).toBe("elk");
      expect(options.elk?.["elk.hierarchyHandling"]).toBe("INCLUDE_CHILDREN");
    });

    it("should allow changing hierarchyHandling setting", () => {
      layoutManager.setLayout("elk");
      layoutManager.updateSetting("elk.hierarchyHandling", "SEPARATE_CHILDREN");

      const options = layoutManager.getLayoutOptions();
      expect(options.elk?.["elk.hierarchyHandling"]).toBe("SEPARATE_CHILDREN");
    });

    it("should apply layout with compound nodes without error", () => {
      layoutManager.setLayout("elk");

      // This should not throw an error
      expect(() => {
        layoutManager.applyLayout(false);
      }).not.toThrow();

      // Verify layout was called with correct options
      expect(mockCy.layout).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "elk",
          elk: expect.objectContaining({
            "elk.hierarchyHandling": "INCLUDE_CHILDREN",
          }),
        }),
      );
    });

    it("should include all ELK options when applying layout", () => {
      layoutManager.setLayout("elk");
      layoutManager.applyLayout(false);

      const layoutCall = mockCy.layout.mock.calls[0][0];

      // Verify all standard ELK options are present
      expect(layoutCall.elk).toBeDefined();
      expect(layoutCall.elk.algorithm).toBeDefined();
      expect(layoutCall.elk["elk.direction"]).toBeDefined();
      expect(layoutCall.elk["elk.spacing.nodeNode"]).toBeDefined();
      expect(layoutCall.elk["elk.hierarchyHandling"]).toBeDefined();
    });
  });

  describe("ELK algorithm variations", () => {
    it("should work with layered algorithm (default)", () => {
      layoutManager.setLayout("elk");
      layoutManager.updateSetting("algorithm", "layered");

      layoutManager.applyLayout(false);

      const options = mockCy.layout.mock.calls[0][0];
      expect(options.elk.algorithm).toBe("layered");
      expect(options.elk["elk.hierarchyHandling"]).toBe("INCLUDE_CHILDREN");
    });

    it("should work with other algorithms (force, stress, mrtree)", () => {
      const algorithms = ["force", "stress", "mrtree"];

      for (const algorithm of algorithms) {
        layoutManager.setLayout("elk");
        layoutManager.updateSetting("algorithm", algorithm);

        layoutManager.applyLayout(false);

        const options =
          mockCy.layout.mock.calls[mockCy.layout.mock.calls.length - 1][0];
        expect(options.elk.algorithm).toBe(algorithm);
        expect(options.elk["elk.hierarchyHandling"]).toBe("INCLUDE_CHILDREN");
      }
    });
  });
});

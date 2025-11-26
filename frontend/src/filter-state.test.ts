import { describe, it, expect, beforeEach, vi } from 'vitest';
import { FilterState } from './filter-state';

// Create mock GraphProcessor directly without importing WASM
function createMockGraphProcessor() {
  return {
    filter_nodes: vi.fn(() => ({
      visible: ['module_a', 'module_b'],
      highlighted: ['module_a', 'module_b'],
    })),
    compute_all_distances: vi.fn(() => ({})),
    get_upstream: vi.fn(() => []),
    get_downstream: vi.fn(() => []),
  };
}

// Mock Cytoscape Core
function createMockCytoscape() {
  const mockNodes = [
    { id: () => 'module_a', style: vi.fn(), data: vi.fn() },
    { id: () => 'module_b', style: vi.fn(), data: vi.fn() },
  ];
  const mockEdges = [
    {
      source: () => ({ id: () => 'module_a' }),
      target: () => ({ id: () => 'module_b' }),
      style: vi.fn(),
    },
  ];

  return {
    nodes: vi.fn(() => mockNodes),
    edges: vi.fn(() => mockEdges),
  } as any;
}

describe('FilterState', () => {
  let filterState: FilterState;
  let mockProcessor: any;
  let mockCy: any;

  beforeEach(() => {
    mockProcessor = createMockGraphProcessor();
    mockCy = createMockCytoscape();
    filterState = new FilterState(mockProcessor, mockCy);
  });

  describe('initialization', () => {
    it('should initialize with default config', () => {
      const config = filterState.getConfig();
      expect(config.showOrphans).toBe(true);
      expect(config.showNamespaces).toBe(true);
      expect(config.excludePatterns).toEqual([]);
      expect(config.maxDistance).toBe(null);
      expect(config.highlightedOnly).toBe(true);
    });
  });

  describe('config updates', () => {
    it('should toggle orphan visibility', () => {
      filterState.toggleOrphans(false);
      expect(filterState.getConfig().showOrphans).toBe(false);
    });

    it('should toggle namespace visibility', () => {
      filterState.toggleNamespaces(false);
      expect(filterState.getConfig().showNamespaces).toBe(false);
    });

    it('should set exclude patterns', () => {
      const patterns = ['test*', '*backup'];
      filterState.setExcludePatterns(patterns);
      expect(filterState.getConfig().excludePatterns).toEqual(patterns);
    });

    it('should set max distance', () => {
      filterState.setMaxDistance(5);
      expect(filterState.getConfig().maxDistance).toBe(5);
    });
  });

  describe('upstream/downstream roots', () => {
    it('should add upstream root', () => {
      filterState.addUpstreamRoot('module_a');
      expect(filterState.getUpstreamRoots()).toContain('module_a');
    });

    it('should remove upstream root', () => {
      filterState.addUpstreamRoot('module_a');
      filterState.removeUpstreamRoot('module_a');
      expect(filterState.getUpstreamRoots()).not.toContain('module_a');
    });

    it('should clear all upstream roots', () => {
      filterState.addUpstreamRoot('module_a');
      filterState.addUpstreamRoot('module_b');
      filterState.clearUpstreamRoots();
      expect(filterState.getUpstreamRoots()).toEqual([]);
    });
  });

  describe('applyFilters', () => {
    it('should call WASM filter_nodes with correct config', () => {
      filterState.applyFilters();

      expect(mockProcessor.filter_nodes).toHaveBeenCalledWith(
        expect.stringContaining('"showOrphans":true')
      );
    });

    it('should update Cytoscape node visibility', () => {
      filterState.applyFilters();

      const nodes = mockCy.nodes();
      nodes.forEach((node: any) => {
        expect(node.style).toHaveBeenCalledWith('display', 'element');
      });
    });

    it('should hide nodes not in visible set', () => {
      // Mock filter_nodes to return only one node
      mockProcessor.filter_nodes.mockReturnValue({
        visible: ['module_a'],
        highlighted: ['module_a'],
      });

      filterState.applyFilters();

      const nodes = mockCy.nodes();
      expect(nodes[0].style).toHaveBeenCalledWith('display', 'element');
      expect(nodes[1].style).toHaveBeenCalledWith('display', 'none');
    });
  });

  describe('highlightedOnly behavior', () => {
    it('should show all nodes when highlightedOnly=true with no filters or CLI highlighting', () => {
      // This tests the exact bug scenario:
      // 1. Default state: highlightedOnly=true (checkbox checked)
      // 2. No upstream/downstream filters active
      // 3. No CLI highlighting in graph data
      // 4. User presses "Apply Filters" button
      // Expected: All nodes should remain visible

      // Mock WASM to simulate: no interactive filters, no CLI highlighting
      mockProcessor.filter_nodes.mockImplementation((configJson: string) => {
        const config = JSON.parse(configJson);

        // Verify the config being passed
        expect(config.upstreamRoots).toEqual([]);
        expect(config.downstreamRoots).toEqual([]);
        expect(config.highlightedOnly).toBe(true);

        // Return all nodes as visible (WASM layer should return this)
        return {
          visible: ['module_a', 'module_b'],
          highlighted: [],
        };
      });

      // Apply filters with default config
      filterState.applyFilters();

      // Verify all nodes are visible
      const nodes = mockCy.nodes();
      expect(nodes[0].style).toHaveBeenCalledWith('display', 'element');
      expect(nodes[1].style).toHaveBeenCalledWith('display', 'element');

      // Verify no nodes are highlighted
      expect(nodes[0].data).toHaveBeenCalledWith('highlighted', false);
      expect(nodes[1].data).toHaveBeenCalledWith('highlighted', false);
    });

    it('should show all nodes but highlight filtered when highlightedOnly=false with upstream filter', () => {
      // Toggle highlightedOnly off
      filterState.toggleHighlightedOnly(false);
      filterState.addUpstreamRoot('module_a');

      // Mock WASM to return: all nodes visible, only upstream highlighted
      mockProcessor.filter_nodes.mockReturnValue({
        visible: ['module_a', 'module_b'],  // All nodes
        highlighted: ['module_a'],  // Only upstream
      });

      filterState.applyFilters();

      const nodes = mockCy.nodes();
      // All nodes visible
      expect(nodes[0].style).toHaveBeenCalledWith('display', 'element');
      expect(nodes[1].style).toHaveBeenCalledWith('display', 'element');

      // Only module_a highlighted
      expect(nodes[0].data).toHaveBeenCalledWith('highlighted', true);
      expect(nodes[1].data).toHaveBeenCalledWith('highlighted', false);
    });

    it('should show only filtered nodes when highlightedOnly=true with upstream filter', () => {
      filterState.addUpstreamRoot('module_a');

      // Mock WASM to return: only upstream nodes visible and highlighted
      mockProcessor.filter_nodes.mockReturnValue({
        visible: ['module_a'],  // Only upstream
        highlighted: ['module_a'],
      });

      filterState.applyFilters();

      const nodes = mockCy.nodes();
      expect(nodes[0].style).toHaveBeenCalledWith('display', 'element');
      expect(nodes[1].style).toHaveBeenCalledWith('display', 'none');

      expect(nodes[0].data).toHaveBeenCalledWith('highlighted', true);
    });
  });
});

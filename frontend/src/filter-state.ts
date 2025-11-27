import type cytoscape from 'cytoscape';
import type { FilterConfig, FilterResult } from './types';
import type { GraphProcessor } from './wasm/deptree_wasm';

export class FilterState {
  private processor: GraphProcessor;
  private cy: cytoscape.Core;
  private config: FilterConfig;

  constructor(processor: GraphProcessor, cy: cytoscape.Core) {
    this.processor = processor;
    this.cy = cy;
    this.config = this.getDefaultConfig();
  }

  /**
   * Get default filter configuration
   */
  private getDefaultConfig(): FilterConfig {
    return {
      showOrphans: true,
      showNamespaces: true,
      excludePatterns: [],
      upstreamRoots: new Set<string>(),
      downstreamRoots: new Set<string>(),
      maxDistance: null,
      highlightedOnly: true,
    };
  }

  /**
   * Reset filters to default
   */
  reset(): void {
    this.config = this.getDefaultConfig();
  }

  /**
   * Update filter configuration
   */
  updateConfig(updates: Partial<FilterConfig>): void {
    this.config = {
      ...this.config,
      ...updates,
    };
  }

  /**
   * Get current filter configuration
   */
  getConfig(): FilterConfig {
    return { ...this.config };
  }

  /**
   * Apply all filters using WASM
   */
  applyFilters(): void {
    console.log('Frontend applyFilters called');

    // Prepare filter configuration for WASM
    const wasmFilterConfig = {
      showOrphans: this.config.showOrphans,
      showNamespaces: this.config.showNamespaces,
      excludePatterns: this.config.excludePatterns,
      upstreamRoots: Array.from(this.config.upstreamRoots),
      downstreamRoots: Array.from(this.config.downstreamRoots),
      maxDistance: this.config.maxDistance,
      highlightedOnly: this.config.highlightedOnly,
    };

    console.log('Filter config:', wasmFilterConfig);

    // Call WASM to compute visible and highlighted nodes
    const result: FilterResult = this.processor.filter_nodes(JSON.stringify(wasmFilterConfig)) as FilterResult;

    console.log('WASM result:', result);

    // Create sets for O(1) lookup
    const visibleSet = new Set(result.visible);
    const highlightedSet = new Set(result.highlighted);

    // Update Cytoscape node visibility
    this.cy.nodes().forEach((node) => {
      const isVisible = visibleSet.has(node.id());
      const isParent = typeof node.isParent === 'function' && node.isParent();

      // For parent nodes, check if ANY child is visible
      if (isParent) {
        const children = typeof node.children === 'function' ? node.children() : [];
        const hasVisibleChildren = typeof (children as any).some === 'function'
          ? (children as any).some((child: { id: () => string }) => visibleSet.has(child.id()))
          : false;
        node.style('display', (isVisible || hasVisibleChildren) ? 'element' : 'none');
      } else {
        node.style('display', isVisible ? 'element' : 'none');
      }
    });

    // Update Cytoscape node highlighting by directly setting styles
    this.cy.nodes().forEach((node) => {
      const nodeId = node.id();
      const shouldHighlight = highlightedSet.has(nodeId);

      // Always set highlighted flag for downstream consumers/tests
      node.data('highlighted', shouldHighlight);
      if (shouldHighlight) {
        console.log(`Setting ${nodeId} as highlighted`);
        // Directly set highlight styles to ensure they're applied
        node.style({
          'background-color': '#ffeb3b',
          'border-width': 4,
          'border-color': '#f57f17',
        });
      } else {
        console.log(`Removing highlight from ${nodeId}`);
        // Remove the inline styles to fall back to stylesheet defaults when supported
        if (typeof (node as any).removeStyle === 'function') {
          node.removeStyle('background-color border-width border-color');
        }
      }
    });

    // Verify what happened
    console.log('After highlighting update:');
    this.cy.nodes().forEach((node) => {
      const nodeStyle = node.style('background-color');
      console.log(`  ${node.id()}: highlighted=${node.data('highlighted')}, bg=${nodeStyle}`);
    });

    // Update edge visibility (only show if both source and target are visible)
    this.cy.edges().forEach((edge) => {
      const sourceVisible = visibleSet.has(edge.source().id());
      const targetVisible = visibleSet.has(edge.target().id());
      const isVisible = sourceVisible && targetVisible;
      edge.style('display', isVisible ? 'element' : 'none');
    });
  }

  /**
   * Toggle orphan node visibility
   */
  toggleOrphans(show: boolean): void {
    this.config.showOrphans = show;
  }

  /**
   * Toggle namespace package visibility
   */
  toggleNamespaces(show: boolean): void {
    this.config.showNamespaces = show;
  }

  /**
   * Toggle highlighted-only mode
   */
  toggleHighlightedOnly(enabled: boolean): void {
    this.config.highlightedOnly = enabled;
  }

  /**
   * Set exclude patterns
   */
  setExcludePatterns(patterns: string[]): void {
    this.config.excludePatterns = patterns;
  }

  /**
   * Set max distance filter
   */
  setMaxDistance(distance: number | null): void {
    this.config.maxDistance = distance;
  }

  /**
   * Add upstream root
   */
  addUpstreamRoot(nodeId: string): void {
    this.config.upstreamRoots.add(nodeId);
  }

  /**
   * Remove upstream root
   */
  removeUpstreamRoot(nodeId: string): void {
    this.config.upstreamRoots.delete(nodeId);
  }

  /**
   * Clear all upstream roots
   */
  clearUpstreamRoots(): void {
    this.config.upstreamRoots.clear();
  }

  /**
   * Add downstream root
   */
  addDownstreamRoot(nodeId: string): void {
    this.config.downstreamRoots.add(nodeId);
  }

  /**
   * Remove downstream root
   */
  removeDownstreamRoot(nodeId: string): void {
    this.config.downstreamRoots.delete(nodeId);
  }

  /**
   * Clear all downstream roots
   */
  clearDownstreamRoots(): void {
    this.config.downstreamRoots.clear();
  }

  /**
   * Get all upstream roots
   */
  getUpstreamRoots(): string[] {
    return Array.from(this.config.upstreamRoots);
  }

  /**
   * Get all downstream roots
   */
  getDownstreamRoots(): string[] {
    return Array.from(this.config.downstreamRoots);
  }
}

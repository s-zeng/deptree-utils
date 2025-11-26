import type cytoscape from 'cytoscape';
import type { FilterConfig } from './types';
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
      highlightedOnly: false,
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

    // Call WASM to compute visible nodes
    const visibleNodesJson = this.processor.filter_nodes(JSON.stringify(wasmFilterConfig));
    const visibleNodes: string[] = JSON.parse(visibleNodesJson);

    // Create a set for O(1) lookup
    const visibleSet = new Set(visibleNodes);

    // Update Cytoscape node visibility
    this.cy.nodes().forEach((node) => {
      const isVisible = visibleSet.has(node.id());
      node.style('display', isVisible ? 'element' : 'none');
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

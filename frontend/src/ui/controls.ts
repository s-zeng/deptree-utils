import type cytoscape from 'cytoscape';
import { cytoscapeControls } from '../cytoscape-manager';
import type { LayoutManager } from '../layout-manager';
import type { FilterState } from '../filter-state';

/**
 * Setup all UI event handlers
 */
export function setupUIEventHandlers(
  cy: cytoscape.Core,
  layoutManager: LayoutManager,
  filterState: FilterState
): void {
  // === Top Control Bar ===

  // Fit to screen
  const fitBtn = document.getElementById('fit');
  if (fitBtn) {
    fitBtn.addEventListener('click', () => {
      cytoscapeControls.fitGraph(cy);
    });
  }

  // Reset zoom
  const resetZoomBtn = document.getElementById('reset-zoom');
  if (resetZoomBtn) {
    resetZoomBtn.addEventListener('click', () => {
      cytoscapeControls.resetZoom(cy);
    });
  }

  // Center graph
  const centerBtn = document.getElementById('center');
  if (centerBtn) {
    centerBtn.addEventListener('click', () => {
      cytoscapeControls.centerGraph(cy);
    });
  }

  // Export PNG
  const exportBtn = document.getElementById('export-png');
  if (exportBtn) {
    exportBtn.addEventListener('click', () => {
      cytoscapeControls.exportPNG(cy);
    });
  }

  // Toggle filter panel
  const toggleFiltersBtn = document.getElementById('toggle-filters');
  if (toggleFiltersBtn) {
    toggleFiltersBtn.addEventListener('click', () => {
      const panel = document.getElementById('filter-panel');
      if (panel) {
        panel.classList.toggle('collapsed');
        // Resize graph after animation
        setTimeout(() => cy.resize(), 300);
      }
    });
  }

  // === Filter Controls ===

  // Show orphans checkbox
  const showOrphansCheckbox = document.getElementById('show-orphans') as HTMLInputElement;
  if (showOrphansCheckbox) {
    showOrphansCheckbox.addEventListener('change', (e) => {
      filterState.toggleOrphans((e.target as HTMLInputElement).checked);
    });
  }

  // Show namespaces checkbox
  const showNamespacesCheckbox = document.getElementById('show-namespaces') as HTMLInputElement;
  if (showNamespacesCheckbox) {
    showNamespacesCheckbox.addEventListener('change', (e) => {
      filterState.toggleNamespaces((e.target as HTMLInputElement).checked);
    });
  }

  // Highlighted only checkbox
  const highlightedOnlyCheckbox = document.getElementById('highlighted-only') as HTMLInputElement;
  if (highlightedOnlyCheckbox) {
    highlightedOnlyCheckbox.addEventListener('change', (e) => {
      filterState.toggleHighlightedOnly((e.target as HTMLInputElement).checked);
    });
  }

  // Distance slider
  const distanceSlider = document.getElementById('distance-slider') as HTMLInputElement;
  const distanceValue = document.getElementById('distance-value');
  if (distanceSlider && distanceValue) {
    distanceSlider.addEventListener('input', (e) => {
      const value = parseInt((e.target as HTMLInputElement).value);
      if (value >= 10) {
        distanceValue.textContent = '∞';
        filterState.setMaxDistance(null);
      } else {
        distanceValue.textContent = value.toString();
        filterState.setMaxDistance(value);
      }
    });
  }

  // Exclude patterns input
  const excludePatternsInput = document.getElementById('exclude-patterns') as HTMLInputElement;
  if (excludePatternsInput) {
    let debounceTimer: number;
    excludePatternsInput.addEventListener('input', (e) => {
      clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(() => {
        const value = (e.target as HTMLInputElement).value;
        const patterns = value
          .split(',')
          .map((p) => p.trim())
          .filter((p) => p.length > 0);
        filterState.setExcludePatterns(patterns);
      }, 300);
    });
  }

  // Apply filters button
  const applyFiltersBtn = document.getElementById('apply-filters');
  if (applyFiltersBtn) {
    applyFiltersBtn.addEventListener('click', () => {
      filterState.applyFilters();
      layoutManager.applyLayout(true);
    });
  }

  // Reset filters button
  const resetFiltersBtn = document.getElementById('reset-filters');
  if (resetFiltersBtn) {
    resetFiltersBtn.addEventListener('click', () => {
      filterState.reset();

      // Reset UI elements
      if (showOrphansCheckbox) showOrphansCheckbox.checked = true;
      if (showNamespacesCheckbox) showNamespacesCheckbox.checked = true;
      if (highlightedOnlyCheckbox) highlightedOnlyCheckbox.checked = false;
      if (distanceSlider) {
        distanceSlider.value = '10';
        if (distanceValue) distanceValue.textContent = '∞';
      }
      if (excludePatternsInput) excludePatternsInput.value = '';

      // Show all nodes
      cy.nodes().style('display', 'element');
      cy.edges().style('display', 'element');

      // Re-apply layout
      layoutManager.applyLayout(true);
    });
  }

  // === Layout Controls ===

  const layoutSelect = document.getElementById('layout-select') as HTMLSelectElement;
  if (layoutSelect) {
    layoutSelect.addEventListener('change', (e) => {
      const selectedLayout = (e.target as HTMLSelectElement).value;
      layoutManager.setLayout(selectedLayout);
      layoutManager.renderSettingsUI();
    });
  }

  const applyLayoutBtn = document.getElementById('apply-layout');
  if (applyLayoutBtn) {
    applyLayoutBtn.addEventListener('click', () => {
      layoutManager.applyLayout(true);
    });
  }
}

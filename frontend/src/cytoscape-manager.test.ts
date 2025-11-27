import { describe, it, expect } from 'vitest';
import { getCytoscapeStyles, HIGHLIGHT_SELECTOR } from './cytoscape-manager';

describe('cytoscape-manager styles', () => {
  it('uses a truthy selector for highlighted nodes so false values are not styled', () => {
    const styles = getCytoscapeStyles();
    const highlighted = styles.find((style) => style.selector === HIGHLIGHT_SELECTOR);

    expect(highlighted).toBeDefined();
    expect(highlighted?.selector).toBe(HIGHLIGHT_SELECTOR);
  });
});

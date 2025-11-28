import { describe, it, expect, beforeEach, vi } from "vitest";
import { LayoutManager } from "./layout-manager";

function createMockCytoscape() {
  return {
    layout: vi.fn(() => ({
      run: vi.fn(),
      stop: vi.fn(),
    })),
  } as any;
}

describe("LayoutManager", () => {
  let layoutManager: LayoutManager;
  let mockCy: any;

  beforeEach(() => {
    mockCy = createMockCytoscape();
    layoutManager = new LayoutManager(mockCy);
  });

  it("should initialize with default layout (dagre)", () => {
    const options = layoutManager.getLayoutOptions();
    expect(options.name).toBe("dagre");
  });

  it("should apply layout to Cytoscape", () => {
    layoutManager.applyLayout(false);
    expect(mockCy.layout).toHaveBeenCalled();
  });

  it("should switch to different layout", () => {
    layoutManager.setLayout("cose");
    const options = layoutManager.getLayoutOptions();
    expect(options.name).toBe("cose");
  });

  it("should get layout options without animation by default", () => {
    const options = layoutManager.getLayoutOptions();
    expect(options.animate).toBe(false);
  });

  it("should get layout options with animation when requested", () => {
    const options = layoutManager.getLayoutOptionsWithAnimation();
    expect(options.animate).toBe(true);
    expect(options.animationDuration).toBe(500);
  });

  it("should apply layout with animation when animated=true", () => {
    layoutManager.applyLayout(true);

    const layoutCall = mockCy.layout.mock.calls[0][0];
    expect(layoutCall.animate).toBe(true);
  });

  it("should apply layout without animation when animated=false", () => {
    layoutManager.applyLayout(false);

    const layoutCall = mockCy.layout.mock.calls[0][0];
    expect(layoutCall.animate).toBe(false);
  });

  it("should update setting value", () => {
    layoutManager.updateSetting("padding", 100);

    const options = layoutManager.getLayoutOptions();
    expect(options.padding).toBe(100);
  });
});

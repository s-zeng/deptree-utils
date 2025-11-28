import type cytoscape from "cytoscape";
import { LAYOUT_CONFIGS, type LayoutSetting } from "./layout-configs";
import type { LayoutOptionsWithExtensions } from "./layout-types";

export class LayoutManager {
  private cy: cytoscape.Core;
  private currentLayout: string = "dagre";
  private settings: Map<string, Map<string, any>>;
  private advancedExpanded: boolean = false;

  constructor(cy: cytoscape.Core) {
    this.cy = cy;
    this.settings = new Map();

    // Initialize default settings for all layouts
    for (const [layoutName, config] of Object.entries(LAYOUT_CONFIGS)) {
      const layoutSettings = new Map<string, any>();

      // Add key settings
      for (const [settingName, setting] of Object.entries(
        config.settings.key,
      )) {
        layoutSettings.set(settingName, setting.default);
      }

      // Add advanced settings
      for (const [settingName, setting] of Object.entries(
        config.settings.advanced,
      )) {
        layoutSettings.set(settingName, setting.default);
      }

      this.settings.set(layoutName, layoutSettings);
    }
  }

  /**
   * Set the current layout
   */
  setLayout(layoutName: string): void {
    if (!LAYOUT_CONFIGS[layoutName]) {
      console.error(`Unknown layout: ${layoutName}`);
      return;
    }
    this.currentLayout = layoutName;
  }

  /**
   * Get current layout options for Cytoscape
   */
  getLayoutOptions(): LayoutOptionsWithExtensions {
    const layoutSettings = this.settings.get(this.currentLayout);
    if (!layoutSettings) {
      return { name: this.currentLayout };
    }

    // ELK uses a nested `elk` options object; keep it separate so Cytoscape-ELK
    // forwards the hierarchy handling and other options correctly for compound graphs.
    if (this.currentLayout === "elk") {
      const elkOptions: Record<string, unknown> = {};

      for (const [key, value] of layoutSettings.entries()) {
        if (value !== null) {
          elkOptions[key] = value;
        }
      }

      return {
        name: "elk",
        animate: false,
        nodeDimensionsIncludeLabels: true,
        elk: elkOptions,
      };
    }

    const options: LayoutOptionsWithExtensions = {
      name: this.currentLayout,
      animate: false,
    };

    // Apply all settings (filter out null values for nullable fields)
    for (const [key, value] of layoutSettings.entries()) {
      if (value !== null) {
        options[key] = value;
      }
    }

    return options;
  }

  /**
   * Get layout options with animation
   */
  getLayoutOptionsWithAnimation(): LayoutOptionsWithExtensions {
    return {
      ...this.getLayoutOptions(),
      animate: true,
      animationDuration: 500,
    };
  }

  /**
   * Apply the current layout
   */
  applyLayout(animated: boolean = true): void {
    const options = animated
      ? this.getLayoutOptionsWithAnimation()
      : this.getLayoutOptions();

    const elements = this.cy.elements(":visible");
    if (elements.length === 0) {
      return;
    }

    elements.layout(options).run();
  }

  /**
   * Update a setting value
   */
  updateSetting(settingName: string, value: any): void {
    const layoutSettings = this.settings.get(this.currentLayout);
    if (layoutSettings) {
      layoutSettings.set(settingName, value);
    }
  }

  /**
   * Render the settings UI for the current layout
   */
  renderSettingsUI(): void {
    const container = document.getElementById("layout-settings-container");
    if (!container) return;

    const config = LAYOUT_CONFIGS[this.currentLayout];
    if (!config) return;

    let html = '<div class="layout-settings">';

    // Render key settings (always visible)
    html += '<div class="key-settings">';
    for (const [settingName, setting] of Object.entries(config.settings.key)) {
      html += this.renderSettingControl(settingName, setting);
    }
    html += "</div>";

    // Render advanced settings (collapsible)
    if (Object.keys(config.settings.advanced).length > 0) {
      html += '<div class="advanced-settings-section">';
      html += `
        <div class="advanced-toggle" id="advanced-toggle">
          <span class="toggle-icon">${this.advancedExpanded ? "▼" : "▶"}</span>
          <span>Advanced Settings</span>
        </div>
      `;

      html += `<div class="advanced-settings-content" style="display: ${
        this.advancedExpanded ? "block" : "none"
      }">`;

      for (const [settingName, setting] of Object.entries(
        config.settings.advanced,
      )) {
        html += this.renderSettingControl(settingName, setting);
      }

      html += "</div>";
      html += "</div>";
    }

    html += "</div>";

    container.innerHTML = html;

    // Attach event listeners
    this.attachSettingListeners();
  }

  /**
   * Render a single setting control
   */
  private renderSettingControl(
    settingName: string,
    setting: LayoutSetting,
  ): string {
    const currentValue =
      this.settings.get(this.currentLayout)?.get(settingName) ??
      setting.default;

    let html = '<div class="layout-setting">';
    html += `<label>${setting.label}</label>`;

    switch (setting.type) {
      case "select":
        html += `<select data-setting="${settingName}">`;
        for (const option of setting.options || []) {
          const selected = currentValue === option.value ? "selected" : "";
          html += `<option value="${option.value}" ${selected}>${option.label}</option>`;
        }
        html += "</select>";
        break;

      case "number":
        const valueAttr =
          currentValue === null || currentValue === undefined
            ? ""
            : `value="${currentValue}"`;
        html += `<input type="number"
          data-setting="${settingName}"
          min="${setting.min}"
          max="${setting.max}"
          step="${setting.step}"
          ${valueAttr}
          ${setting.nullable ? 'placeholder="Auto"' : ""}
        />`;
        break;

      case "checkbox":
        const checked = currentValue ? "checked" : "";
        html += `<input type="checkbox"
          data-setting="${settingName}"
          ${checked}
        />`;
        break;
    }

    html += "</div>";
    return html;
  }

  /**
   * Attach event listeners to setting controls
   */
  private attachSettingListeners(): void {
    const container = document.getElementById("layout-settings-container");
    if (!container) return;

    // Setting change listeners
    container.querySelectorAll("[data-setting]").forEach((element) => {
      const settingName = (element as HTMLElement).dataset.setting;
      if (!settingName) return;

      if (element instanceof HTMLSelectElement) {
        element.addEventListener("change", (e) => {
          const value = (e.target as HTMLSelectElement).value;
          this.updateSetting(settingName, value);
        });
      } else if (element instanceof HTMLInputElement) {
        if (element.type === "checkbox") {
          element.addEventListener("change", (e) => {
            const value = (e.target as HTMLInputElement).checked;
            this.updateSetting(settingName, value);
          });
        } else if (element.type === "number") {
          element.addEventListener("change", (e) => {
            const inputValue = (e.target as HTMLInputElement).value;
            const value = inputValue === "" ? null : parseFloat(inputValue);
            this.updateSetting(settingName, value);
          });
        }
      }
    });

    // Advanced toggle listener
    const advancedToggle = document.getElementById("advanced-toggle");
    if (advancedToggle) {
      advancedToggle.addEventListener("click", () => {
        this.advancedExpanded = !this.advancedExpanded;
        this.renderSettingsUI();
      });
    }
  }
}

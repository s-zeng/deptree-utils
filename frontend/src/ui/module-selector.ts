/**
 * Module selector component for choosing modules for upstream/downstream filtering
 */
export class ModuleSelector {
  private container: HTMLElement;
  private allModules: string[];
  private selected: Set<string>;
  private onChange: (selected: Set<string>) => void;

  constructor(
    containerId: string,
    allModules: string[],
    onChange: (selected: Set<string>) => void,
  ) {
    const container = document.getElementById(containerId);
    if (!container) {
      throw new Error(`Element with id ${containerId} not found`);
    }
    this.container = container;
    this.allModules = allModules;
    this.selected = new Set();
    this.onChange = onChange;
  }

  /**
   * Render the module selector UI
   */
  render(): void {
    const chips = Array.from(this.selected)
      .sort()
      .map(
        (m) =>
          `<span class="module-chip">${m} <span class="remove" data-module="${m}">×</span></span>`,
      )
      .join("");

    this.container.innerHTML = chips;

    // Add remove handlers
    this.container.querySelectorAll(".remove").forEach((btn) => {
      btn.addEventListener("click", () => {
        const module = (btn as HTMLElement).dataset.module;
        if (module) {
          this.removeModule(module);
        }
      });
    });
  }

  /**
   * Add a module to the selection
   */
  addModule(moduleId: string): void {
    this.selected.add(moduleId);
    this.render();
    this.onChange(this.selected);
  }

  /**
   * Remove a module from the selection
   */
  removeModule(moduleId: string): void {
    this.selected.delete(moduleId);
    this.render();
    this.onChange(this.selected);
  }

  /**
   * Clear all selected modules
   */
  clear(): void {
    this.selected.clear();
    this.render();
  }

  /**
   * Get all selected modules
   */
  getSelected(): Set<string> {
    return new Set(this.selected);
  }
}

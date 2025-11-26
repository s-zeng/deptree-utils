/* tslint:disable */
/* eslint-disable */
/**
 * Main graph processor exposed to JavaScript
 */
export class GraphProcessor {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new GraphProcessor from JSON
   */
  constructor(graph_json: string);
  /**
   * Compute all-pairs shortest paths using BFS
   * Returns JSON object with distances: { "node1": { "node2": 2, "node3": 1 }, ... }
   */
  compute_all_distances(): any;
  /**
   * Check if a node is an orphan (no incoming or outgoing edges)
   */
  is_orphan(node_id: string): boolean;
  /**
   * Filter nodes based on criteria
   * Returns JSON object with both visible and highlighted node IDs
   */
  filter_nodes(filter_config_json: string): any;
  /**
   * Get all upstream dependencies from given roots
   * Returns JSON array of node IDs
   */
  get_upstream(roots: string[], max_distance?: number | null): any;
  /**
   * Get all downstream dependents from given roots
   * Returns JSON array of node IDs
   */
  get_downstream(roots: string[], max_distance?: number | null): any;
  /**
   * Return the graph configuration supplied by the CLI (if any)
   */
  get_config(): any;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_graphprocessor_free: (a: number, b: number) => void;
  readonly graphprocessor_new: (a: number, b: number) => [number, number, number];
  readonly graphprocessor_compute_all_distances: (a: number) => any;
  readonly graphprocessor_is_orphan: (a: number, b: number, c: number) => number;
  readonly graphprocessor_filter_nodes: (a: number, b: number, c: number) => any;
  readonly graphprocessor_get_upstream: (a: number, b: number, c: number, d: number) => any;
  readonly graphprocessor_get_downstream: (a: number, b: number, c: number, d: number) => any;
  readonly graphprocessor_get_config: (a: number) => any;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;

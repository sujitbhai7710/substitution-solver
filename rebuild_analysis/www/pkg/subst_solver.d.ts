/* tslint:disable */
/* eslint-disable */
export function solve_js(puzzle: string, clue: string, restarts: number, steps: number): string;
/**
 * Explicit warmup export so the page can init models off the critical path.
 */
export function warmup_js(): void;
/**
 * N-best results for the page: top-N distinct candidates re-ranked by the
 * combined final-selection score, best first. JSON array of
 * {"plaintext","key","score"}.
 */
export function solve_n_js(puzzle: string, clue: string, restarts: number, steps: number, n: number): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly solve_js: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
  readonly solve_n_js: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
  readonly warmup_js: () => void;
  readonly __wbindgen_export_0: WebAssembly.Table;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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

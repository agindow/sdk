/** TI-40 host recovery for synchronous WASM inspection (§10.9).
 * A trapped guest can retain inconsistent state: retire it permanently.
 * Guest creation/replacement and the production SDK ABI belong to EPIC-07.
 */
export class WasmGuard {
  private retired = false;
  private readonly inspect: () => void;

  constructor(inspect: () => void) {
    this.inspect = inspect;
  }

  run<O>(operation: () => O): { status: 'inspected' | 'bypassed'; outcome: O } {
    let status: 'inspected' | 'bypassed' = 'bypassed';
    if (!this.retired) {
      try {
        this.inspect();
        status = 'inspected';
      } catch {
        this.retired = true;
      }
    }
    // Caller errors are outside recovery and must never trigger a retry.
    return { status, outcome: operation() };
  }
}

import First from '@agindow/native-one';
import Second from '@agindow/native-two';
let active = First;
/** Two independently instantiated copies of the same compiled WASM; only the test can switch. */
export function resetNative(): void {
  active = Second;
}
export default class SwitchingNative {
  readonly #host: First;
  constructor(context: unknown, bindings: Record<string, unknown>) {
    this.#host = new active(context, bindings);
  }
  fetch(request: Request): Promise<Response> {
    return this.#host.fetch(request);
  }
}

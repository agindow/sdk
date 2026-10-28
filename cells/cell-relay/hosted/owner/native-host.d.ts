/** Boundary to worker-build's generated WorkerEntrypoint. No HTTP route is added. */
export default class NativeHost {
 constructor(context:unknown,bindings:Record<string,unknown>);
 fetch(request:Request):Promise<Response>;
}

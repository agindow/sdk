import type { Event } from './types.js';
export type EventType = Event['type'];
/** Fixed storage; listeners never receive mutable decision state. */
export class Events {
  private slots: (Event | undefined)[] = [];
  private capacity = 0;
  private next = 0;
  private count = 0;
  private readonly listeners = new Map<EventType, Set<(event: Event) => void>>();
  resize(capacity: number): void {
    if (capacity === this.capacity) return;
    const retained = this.recent().slice(-capacity);
    this.capacity = capacity; this.slots = new Array<Event | undefined>(capacity); this.next = 0; this.count = 0;
    if (capacity > 0) for (const event of retained) this.retain(event);
  }
  private retain(event: Event): void {
    if (this.capacity === 0) return;
    this.slots[this.next] = event; this.next = (this.next + 1) % this.capacity;
    this.count = Math.min(this.count + 1, this.capacity);
  }
  emit(event: Event): void {
    this.retain(event);
    for (const listener of this.listeners.get(event.type) ?? []) {
      try { listener(event); } catch { /* a subscriber cannot alter upstream execution */ }
    }
  }
  recent(): readonly Event[] {
    const result: Event[] = [];
    for (let index = 0; index < this.count; index++) {
      const event = this.slots[(this.next - this.count + index + this.capacity) % this.capacity];
      if (event !== undefined) result.push(event);
    }
    return result;
  }
  on(type: EventType, callback: (event: Event) => void): () => void {
    let listeners = this.listeners.get(type);
    if (listeners === undefined) { listeners = new Set(); this.listeners.set(type, listeners); }
    listeners.add(callback);
    return () => { listeners.delete(callback); };
  }
}

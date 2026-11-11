//! Opt-in DevTools diagnostics: open the arena with ?profile=1.

export class NetworkProfiler {
  private readonly enabled = new URLSearchParams(location.search).get("profile") === "1";
  private since = performance.now();
  private frames: number[] = [];
  private bytes = 0;
  private applyMs = 0;
  private chunks = 0;

  chunk(bytes: number, applyMs: number) {
    if (!this.enabled) return;
    this.bytes += bytes;
    this.applyMs += applyMs;
    this.chunks++;
  }

  frame(ms: number, cells: number) {
    if (!this.enabled) return;
    this.frames.push(ms);
    const now = performance.now();
    const elapsed = now - this.since;
    if (elapsed < 2000) return;
    this.frames.sort((a, b) => a - b);
    console.table({
      cells,
      framesPerSecond: this.frames.length * 1000 / elapsed,
      frameP50Ms: this.frames[Math.floor(this.frames.length * 0.5)],
      frameP95Ms: this.frames[Math.floor(this.frames.length * 0.95)],
      frameMaxMs: this.frames[this.frames.length - 1],
      chunkApplyMeanMs: this.applyMs / Math.max(1, this.chunks),
      inboundBytesPerSecond: this.bytes * 1000 / elapsed,
    });
    this.since = now;
    this.frames.length = 0;
    this.bytes = this.applyMs = this.chunks = 0;
  }
}

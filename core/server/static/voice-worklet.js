// AudioWorklet: inoltra i campioni del microfono al thread principale in blocchi da ~85 ms.
class JarvisCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buffer = [];
    this.size = 0;
  }
  process(inputs) {
    const channel = inputs[0] && inputs[0][0];
    if (channel) {
      this.buffer.push(new Float32Array(channel));
      this.size += channel.length;
      if (this.size >= 4096) {
        const out = new Float32Array(this.size);
        let offset = 0;
        for (const b of this.buffer) { out.set(b, offset); offset += b.length; }
        this.port.postMessage(out, [out.buffer]);
        this.buffer = [];
        this.size = 0;
      }
    }
    return true;
  }
}
registerProcessor("jarvis-capture", JarvisCapture);

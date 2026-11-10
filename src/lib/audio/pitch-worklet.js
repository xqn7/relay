// Pitch shifter for the audio chain: changes pitch without changing speed (speed is the
// <audio> element's own time-stretch). Two read heads sweep through a short delay line at a
// rate that resamples the sound, each faded in and out so the jumps back are never heard.
// Plain JS because it runs as an AudioWorklet module, outside the app's bundle.

const WINDOW = 2048; // samples per sweep (~46 ms at 44.1 kHz): short enough to keep timing tight
const SIZE = 8192; // delay line, a power of two above WINDOW

class PitchShifter extends AudioWorkletProcessor {
  static get parameterDescriptors() {
    return [{ name: "ratio", defaultValue: 1, minValue: 0.5, maxValue: 2, automationRate: "k-rate" }];
  }

  constructor() {
    super();
    this.lines = [];
    this.write = 0;
    this.phase = 0;
  }

  process(inputs, outputs, parameters) {
    const input = inputs[0];
    const output = outputs[0];
    const step = (1 - parameters.ratio[0]) / WINDOW;
    while (this.lines.length < output.length) this.lines.push(new Float32Array(SIZE));
    const mask = SIZE - 1;
    let write = this.write;
    let phase = this.phase;
    for (let n = 0; n < output[0].length; n++) {
      const other = (phase + 0.5) % 1;
      // sin^2 + cos^2 = 1: the two heads always add up to full level.
      const g1 = Math.sin(Math.PI * phase) ** 2;
      const g2 = 1 - g1;
      const p1 = write - (phase * WINDOW + 1);
      const p2 = write - (other * WINDOW + 1);
      const i1 = Math.floor(p1);
      const i2 = Math.floor(p2);
      const f1 = p1 - i1;
      const f2 = p2 - i2;
      for (let c = 0; c < output.length; c++) {
        const line = this.lines[c];
        const src = input[c] ?? input[0];
        line[write & mask] = src ? src[n] : 0;
        const a = line[i1 & mask] + (line[(i1 + 1) & mask] - line[i1 & mask]) * f1;
        const b = line[i2 & mask] + (line[(i2 + 1) & mask] - line[i2 & mask]) * f2;
        output[c][n] = g1 * a + g2 * b;
      }
      write = (write + 1) & mask;
      phase += step;
      phase -= Math.floor(phase);
    }
    this.write = write;
    this.phase = phase;
    return true;
  }
}

registerProcessor("pitch-shifter", PitchShifter);

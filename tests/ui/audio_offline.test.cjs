const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const staticSource = (name) => readFileSync(join(__dirname, "../../core/server/static", name), "utf8");

test("la PWA ripropone la navigazione dopo un guasto, preservando le risposte di accesso", async () => {
  const listeners = {};
  const context = vm.createContext({
    self: { addEventListener: (event, listener) => { listeners[event] = listener; } },
    Response,
  });
  vm.runInContext(staticSource("sw.js"), context);
  const request = { mode: "navigate" };
  const dispatch = (req = request) => {
    let response;
    listeners.fetch({ request: req, respondWith: (value) => { response = value; } });
    return response;
  };
  context.fetch = async (req) => {
    assert.equal(req, request);
    throw new Error("offline");
  };
  const offline = await dispatch();
  assert.equal(offline.status, 503);
  assert.equal(offline.headers.get("Cache-Control"), "no-store");
  assert.match(await offline.text(), /<a href=''>Riprova<\/a>/);
  for (const status of [502, 503, 504, 403, 200]) {
    const upstream = new Response("", { status });
    context.fetch = async () => upstream;
    const result = await dispatch();
    if (status >= 502) assert.equal(result.status, 503);
    else assert.equal(result, upstream);
  }
  assert.equal(dispatch({ mode: "cors" }), undefined);
});

// Il motore vocale è in TypeScript (apps/web/src/lib/voice.ts): lo si traspila con il compilatore della
// dashboard (`npm ci` in apps/web) e lo si esegue con API del browser finte.
function loadVoice(globals) {
  const ts = require(join(__dirname, "../../apps/web/node_modules/typescript"));
  const source = readFileSync(join(__dirname, "../../apps/web/src/lib/voice.ts"), "utf8");
  const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } });
  const module = { exports: {} };
  const context = vm.createContext({ ...globals, module, exports: module.exports, require: (name) => {
    if (name === "./api") return { AUTH: {} };
    throw new Error(`modulo inatteso: ${name}`);
  } });
  vm.runInContext(outputText, context);
  return { context, voice: module.exports.voice };
}

test("il movimento della bocca segue l'audio e si azzera quando si interrompe", async () => {
  const properties = new Map();
  const frames = new Map();
  let id = 0;
  const audio = { currentTime: 0, paused: false, src: "blob:x", pause() { this.paused = true; } };
  const blob = { arrayBuffer: async () => new ArrayBuffer(0) };
  const avatar = { style: { setProperty: (...args) => properties.set(...args), removeProperty: (key) => properties.delete(key) } };
  const { context, voice } = loadVoice({
    document: { querySelectorAll: () => [avatar] },
    OfflineAudioContext: class {
      async decodeAudioData() {
        const samples = new Float32Array(16000);
        samples[0] = 0.125;
        return { sampleRate: 16000, getChannelData: () => samples };
      }
    },
    matchMedia: () => ({ matches: false }),
    requestAnimationFrame: (cb) => { frames.set(++id, cb); return id; },
    cancelAnimationFrame: (frame) => frames.delete(frame),
    URL: { revokeObjectURL() {} },
  });
  voice.player = audio;
  await voice.animateSpeech(audio, blob);
  const tick = () => {
    const [frame, cb] = frames.entries().next().value;
    frames.delete(frame);
    cb();
  };
  tick();
  assert.equal(properties.get("--mouth-opening"), "0.50");
  audio.currentTime = 0.5;
  tick();
  assert.equal(properties.get("--mouth-opening"), "0.00");
  voice.stopSpeaking();
  assert.equal(frames.size, 0);
  assert.equal(properties.has("--mouth-opening"), false);
  context.matchMedia = () => ({ matches: true });
  voice.player = audio;
  await voice.animateSpeech(audio, blob);
  assert.equal(frames.size, 0);
});

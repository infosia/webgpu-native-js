(() => {
  const used = new Set();
  globalThis.__shimLog = name => {
    name = String(name);
    if (!used.has(name)) {
      used.add(name);
      __log_shim(name);
    }
  };

  const log = (name, args) => {
    __shimLog(`console.${name}`);
    print(...args.map(String));
  };
  globalThis.console = {
    log: (...args) => log("log", args),
    info: (...args) => log("info", args),
    warn: (...args) => log("warn", args),
    error: (...args) => log("error", args),
    debug: (...args) => log("debug", args),
  };

  globalThis.performance = {
    now() {
      __shimLog("performance.now");
      return __perf_now();
    },
  };

  let nextTimerId = 1;
  const timers = [];
  const cancelledTimerIds = new Set();
  const pendingTimerIds = new Set();
  const less = (a, b) => a.due < b.due || (a.due === b.due && a.id < b.id);
  const push = timer => {
    timers.push(timer);
    let i = timers.length - 1;
    while (i > 0) {
      const parent = (i - 1) >> 1;
      if (!less(timers[i], timers[parent])) break;
      [timers[i], timers[parent]] = [timers[parent], timers[i]];
      i = parent;
    }
  };
  const pop = () => {
    const first = timers[0];
    const last = timers.pop();
    if (timers.length) {
      timers[0] = last;
      let i = 0;
      for (;;) {
        let child = i * 2 + 1;
        if (child >= timers.length) break;
        if (child + 1 < timers.length && less(timers[child + 1], timers[child])) child++;
        if (!less(timers[child], timers[i])) break;
        [timers[i], timers[child]] = [timers[child], timers[i]];
        i = child;
      }
    }
    return first;
  };
  const schedule = (callback, delay, repeat, args) => {
    if (typeof callback !== "function") throw new TypeError("timer callback must be a function");
    const id = nextTimerId++;
    delay = Math.max(0, Number(delay) || 0);
    __shimLog(repeat ? "setInterval" : "setTimeout");
    pendingTimerIds.add(id);
    push({ id, callback, args, due: __perf_now() + delay, delay, repeat, cancelled: false });
    return id;
  };
  globalThis.setTimeout = (callback, delay = 0, ...args) =>
    schedule(callback, delay, false, args);
  globalThis.setInterval = (callback, delay = 0, ...args) =>
    schedule(callback, delay, true, args);
  const cancel = id => {
    __shimLog("clearTimeout/clearInterval");
    id = Number(id);
    if (!pendingTimerIds.has(id)) return;
    cancelledTimerIds.add(id);
    for (const timer of timers) if (timer.id === id) timer.cancelled = true;
  };
  globalThis.clearTimeout = cancel;
  globalThis.clearInterval = cancel;
  globalThis.__runDueTimers = now => {
    const repeating = [];
    while (timers.length && timers[0].due <= now) {
      const timer = pop();
      const cancelled = cancelledTimerIds.delete(timer.id);
      if (timer.cancelled || cancelled) {
        pendingTimerIds.delete(timer.id);
        continue;
      }
      if (!timer.repeat) pendingTimerIds.delete(timer.id);
      timer.callback(...timer.args);
      if (timer.repeat && !timer.cancelled && !cancelledTimerIds.delete(timer.id)) {
        timer.due = __perf_now() + timer.delay;
        repeating.push(timer);
      } else if (timer.repeat) {
        pendingTimerIds.delete(timer.id);
      }
    }
    for (const timer of repeating) push(timer);
  };

  const scalarValues = value => {
    const string = String(value);
    const values = [];
    for (let i = 0; i < string.length; i++) {
      const first = string.charCodeAt(i);
      if (first >= 0xd800 && first <= 0xdbff && i + 1 < string.length) {
        const second = string.charCodeAt(i + 1);
        if (second >= 0xdc00 && second <= 0xdfff) {
          values.push(0x10000 + ((first - 0xd800) << 10) + second - 0xdc00);
          i++;
          continue;
        }
      }
      values.push(first >= 0xd800 && first <= 0xdfff ? 0xfffd : first);
    }
    return values;
  };
  class TextEncoder {
    get encoding() { return "utf-8"; }
    encode(value = "") {
      __shimLog("TextEncoder.encode");
      const bytes = [];
      for (const cp of scalarValues(value)) {
        if (cp <= 0x7f) bytes.push(cp);
        else if (cp <= 0x7ff) bytes.push(0xc0 | cp >> 6, 0x80 | cp & 0x3f);
        else if (cp <= 0xffff) bytes.push(0xe0 | cp >> 12, 0x80 | cp >> 6 & 0x3f, 0x80 | cp & 0x3f);
        else bytes.push(0xf0 | cp >> 18, 0x80 | cp >> 12 & 0x3f, 0x80 | cp >> 6 & 0x3f, 0x80 | cp & 0x3f);
      }
      return new Uint8Array(bytes);
    }
    encodeInto(source, destination) {
      __shimLog("TextEncoder.encodeInto");
      let read = 0;
      let written = 0;
      const string = String(source);
      while (read < string.length) {
        const first = string.charCodeAt(read);
        const pair = first >= 0xd800 && first <= 0xdbff && read + 1 < string.length &&
          string.charCodeAt(read + 1) >= 0xdc00 && string.charCodeAt(read + 1) <= 0xdfff;
        const bytes = this.encode(string.slice(read, read + (pair ? 2 : 1)));
        if (written + bytes.length > destination.length) break;
        destination.set(bytes, written);
        written += bytes.length;
        read += pair ? 2 : 1;
      }
      return { read, written };
    }
  }
  class TextDecoder {
    constructor(label = "utf-8", options = {}) {
      if (!/^(utf-8|utf8|unicode-1-1-utf-8)$/i.test(String(label))) throw new RangeError("unsupported encoding");
      this.fatal = Boolean(options.fatal);
      this.ignoreBOM = Boolean(options.ignoreBOM);
      this.encoding = "utf-8";
    }
    decode(input = new Uint8Array()) {
      __shimLog("TextDecoder.decode");
      const bytes = input instanceof ArrayBuffer ? new Uint8Array(input) :
        new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
      let result = "";
      const invalid = () => {
        if (this.fatal) throw new TypeError("invalid UTF-8");
        result += "\ufffd";
      };
      for (let i = 0; i < bytes.length;) {
        const b0 = bytes[i++];
        if (b0 <= 0x7f) { result += String.fromCodePoint(b0); continue; }
        let count, cp, minimum;
        if (b0 >= 0xc2 && b0 <= 0xdf) { count = 1; cp = b0 & 0x1f; minimum = 0x80; }
        else if (b0 >= 0xe0 && b0 <= 0xef) { count = 2; cp = b0 & 0x0f; minimum = 0x800; }
        else if (b0 >= 0xf0 && b0 <= 0xf4) { count = 3; cp = b0 & 7; minimum = 0x10000; }
        else { invalid(); continue; }
        if (i + count > bytes.length) { invalid(); break; }
        let valid = true;
        for (let n = 0; n < count; n++) {
          const next = bytes[i + n];
          if ((next & 0xc0) !== 0x80) { valid = false; break; }
          cp = cp << 6 | next & 0x3f;
        }
        if (!valid || cp < minimum || cp > 0x10ffff || cp >= 0xd800 && cp <= 0xdfff) { invalid(); continue; }
        i += count;
        result += String.fromCodePoint(cp);
      }
      if (!this.ignoreBOM && result.charCodeAt(0) === 0xfeff) result = result.slice(1);
      return result;
    }
  }
  globalThis.TextEncoder = TextEncoder;
  globalThis.TextDecoder = TextDecoder;

  globalThis.DOMException = class DOMException extends Error {
    constructor(message = "", name = "Error") {
      super(String(message));
      this.name = String(name);
      __shimLog("DOMException");
    }
  };
  class Event {
    constructor(type) {
      this.type = String(type);
      this.target = null;
      this.currentTarget = null;
    }
  }
  class MessageEvent extends Event {
    constructor(type, init = {}) {
      super(type);
      this.data = init.data ?? null;
      __shimLog("MessageEvent");
    }
  }
  class EventTarget {
    constructor() { this.__listeners = new Map(); }
    addEventListener(type, callback, options = {}) {
      __shimLog("EventTarget");
      if (callback === null || callback === undefined) return;
      type = String(type);
      const listeners = this.__listeners.get(type) ?? [];
      if (!listeners.some(entry => entry.callback === callback)) {
        listeners.push({ callback, once: Boolean(typeof options === "object" && options.once) });
        this.__listeners.set(type, listeners);
      }
    }
    removeEventListener(type, callback) {
      const listeners = this.__listeners.get(String(type));
      if (!listeners) return;
      this.__listeners.set(String(type), listeners.filter(entry => entry.callback !== callback));
    }
    dispatchEvent(event) {
      if (!(event instanceof Event)) throw new TypeError("argument must be an Event");
      event.target = this;
      event.currentTarget = this;
      for (const entry of [...(this.__listeners.get(event.type) ?? [])]) {
        if (typeof entry.callback === "function") entry.callback.call(this, event);
        else entry.callback.handleEvent(event);
        if (entry.once) this.removeEventListener(event.type, entry.callback);
      }
      return true;
    }
  }
  globalThis.Event = Event;
  globalThis.MessageEvent = MessageEvent;
  globalThis.EventTarget = EventTarget;

  // Native interface wrappers have their WebGPU prototypes, but the binding does not
  // install non-constructible WebGPU interface objects on the global object. The CTS
  // fixture uses this one solely to distinguish devices during cleanup.
  globalThis.GPUDevice = class GPUDevice {
    constructor() { throw new TypeError("Illegal constructor"); }
    static [Symbol.hasInstance](value) {
      __shimLog("GPUDevice[Symbol.hasInstance]");
      return typeof value === "object" && value !== null &&
        typeof value.requestDevice === "undefined" &&
        typeof value.createBuffer === "function" &&
        typeof value.pushErrorScope === "function" &&
        "queue" in value && "lost" in value;
    }
  };
  globalThis.navigator = { gpu: globalThis.gpu };
  globalThis.self = globalThis;
})();

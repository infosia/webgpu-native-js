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

  globalThis.queueMicrotask = callback => {
    if (typeof callback !== "function") throw new TypeError("callback must be a function");
    __shimLog("queueMicrotask");
    Promise.resolve().then(() => callback());
  };

  if (!("stack" in new Error("probe"))) {
    Object.defineProperty(Error.prototype, "stack", {
      configurable: true,
      get() {
        __shimLog("Error.prototype.stack");
        return "<stack unavailable: cts-runner shim (engine does not expose Error.prototype.stack)>";
      },
      set(value) {
        Object.defineProperty(this, "stack", {
          configurable: true,
          writable: true,
          value,
        });
      },
    });
  }

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

  const DOMExceptionBase = globalThis.DOMException || class DOMException extends Error {
    constructor(message = "", name = "Error") {
      super(String(message));
      this.name = String(name);
      __shimLog("DOMException");
    }
  };
  globalThis.DOMException = DOMExceptionBase;
  const EventBase = globalThis.Event || class Event {
    constructor(type) {
      this.type = String(type);
      this.target = null;
      this.currentTarget = null;
    }
  }
  class MessageEvent extends EventBase {
    constructor(type, init = {}) {
      super(type);
      this.data = Object.prototype.hasOwnProperty.call(init, "data") ? init.data : null;
      __shimLog("MessageEvent");
    }
  }
  const EventTargetBase = globalThis.EventTarget || class EventTarget {
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
      if (!(event instanceof EventBase)) throw new TypeError("argument must be an Event");
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

  const mappedArrayBuffers = new WeakSet();
  const bufferPrototype = globalThis.GPUBuffer && globalThis.GPUBuffer.prototype;
  if (bufferPrototype) {
    const descriptor = Object.getOwnPropertyDescriptor(bufferPrototype, "getMappedRange");
    if (descriptor && typeof descriptor.value === "function") {
      const original = descriptor.value;
      Object.defineProperty(bufferPrototype, "getMappedRange", {
        ...descriptor,
        value: function getMappedRange() {
          const range = original.apply(this, arguments);
          mappedArrayBuffers.add(range);
          return range;
        },
      });
    }
  }

  const arrayBufferSlice = ArrayBuffer.prototype.slice;
  const dataCloneError = message => new DOMExceptionBase(message, "DataCloneError");
  const isDetachedArrayBuffer = value => {
    try {
      new Uint8Array(value);
      return false;
    } catch (error) {
      return error instanceof TypeError;
    }
  };
  const cloneMessage = (value, transferred) => {
    if (value instanceof ArrayBuffer) {
      return transferred.get(value) ?? arrayBufferSlice.call(value, 0);
    }
    if (value === null || typeof value !== "object") return value;
    throw dataCloneError("MessageChannel cannot clone this value");
  };
  class MessagePort {
    constructor() {
      this.onmessage = null;
      this.__listeners = [];
      this.__peer = null;
    }
    addEventListener(type, callback) {
      if (String(type) !== "message" || callback === null || callback === undefined) return;
      if (!this.__listeners.includes(callback)) this.__listeners.push(callback);
    }
    removeEventListener(type, callback) {
      if (String(type) !== "message") return;
      this.__listeners = this.__listeners.filter(listener => listener !== callback);
    }
    postMessage(value, transferList = []) {
      __shimLog("MessagePort.postMessage");
      const transfers = Array.from(transferList);
      if (value !== null && typeof value === "object" && !(value instanceof ArrayBuffer)) {
        throw dataCloneError("MessageChannel cannot clone this value");
      }
      const seen = new Set();
      for (const transfer of transfers) {
        if (!(transfer instanceof ArrayBuffer)) {
          throw dataCloneError("transfer list item is not an ArrayBuffer");
        }
        if (seen.has(transfer)) throw dataCloneError("duplicate transferable");
        seen.add(transfer);
        if (isDetachedArrayBuffer(transfer)) {
          throw dataCloneError("ArrayBuffer is detached");
        }
        if (mappedArrayBuffers.has(transfer)) {
          throw new TypeError("mapped ArrayBuffer is not transferable");
        }
      }
      const transferred = new Map();
      for (const transfer of transfers) {
        transferred.set(transfer, __transfer_array_buffer(transfer));
      }
      const message = cloneMessage(value, transferred);
      setTimeout(() => this.__peer.__dispatchMessage(message), 0);
    }
    __dispatchMessage(data) {
      const event = new MessageEvent("message", { data });
      event.target = this;
      event.currentTarget = this;
      for (const listener of [...this.__listeners]) {
        if (typeof listener === "function") listener.call(this, event);
        else listener.handleEvent(event);
      }
      if (typeof this.onmessage === "function") this.onmessage.call(this, event);
    }
  }
  class MessageChannel {
    constructor() {
      __shimLog("MessageChannel");
      this.port1 = new MessagePort();
      this.port2 = new MessagePort();
      this.port1.__peer = this.port2;
      this.port2.__peer = this.port1;
    }
  }
  globalThis.Event = EventBase;
  globalThis.MessageEvent = MessageEvent;
  globalThis.EventTarget = EventTargetBase;
  globalThis.MessagePort = MessagePort;
  globalThis.MessageChannel = MessageChannel;

  globalThis.navigator = { gpu: globalThis.gpu };
  globalThis.self = globalThis;
})();

const { nameCodeMap, DOMException } = require('./lib/errors.js');
const {
  isFunction, isPlainObject, isPositiveInt, isPositiveNumber, kEnumerableProperty
} = require('./lib/utils.js');
const { kNapiObj } = require('./lib/symbols.js');
const { bridgeEventTarget } = require('./lib/events.js');

const { kNativeAudioBuffer } = require('./AudioBuffer.js');

// constructor(OfflineAudioContextOptions contextOptions);
// constructor(unsigned long numberOfChannels, unsigned long length, float sampleRate);
// Promise<AudioBuffer> startRendering();
// Promise<undefined> resume();
// Promise<undefined> suspend(double suspendTime);
// readonly attribute unsigned long length;
// attribute EventHandler oncomplete;

module.exports = function patchOfflineAudioContext(jsExport, nativeBinding) {
  class OfflineAudioContext extends jsExport.BaseAudioContext {
    constructor(...args) {
      if (arguments.length < 1) {
        throw new TypeError(`Failed to construct 'OfflineAudioContext': 1 argument required, but only ${arguments.length} present`);
      }

      // handle initialisation with either an options object or a sequence of parameters
      // https://webaudio.github.io/web-audio-api/#dom-offlineaudiocontext-constructor-contextoptions-contextoptions
      if (isPlainObject(args[0]) && 'length' in args[0] && 'sampleRate' in args[0]
      ) {
        let { numberOfChannels, length, sampleRate } = args[0];
        if (numberOfChannels === undefined) {
            numberOfChannels = 1;
        }
        args = [numberOfChannels, length, sampleRate];
      }

      const [numberOfChannels, length, sampleRate] = args;

      if (!isPositiveInt(numberOfChannels)) {
        throw new TypeError(`Failed to construct 'OfflineAudioContext': Invalid value for numberOfChannels: ${numberOfChannels}`);
      } else if (!isPositiveInt(length)) {
        throw new TypeError(`Failed to construct 'OfflineAudioContext': Invalid value for length: ${length}`);
      } else if (!isPositiveNumber(sampleRate)) {
        throw new TypeError(`Failed to construct 'OfflineAudioContext': Invalid value for sampleRate: ${sampleRate}`);
      }

      let napiObj;

      try {
        napiObj = new nativeBinding.OfflineAudioContext(numberOfChannels, length, sampleRate);
      } catch (err) {
        throwSanitizedError(err);
      }

      super(napiObj);
    }

    get length() {
      return this[kNapiObj].length;
    }

    get oncomplete() {
      return this._complete || null;
    }

    set oncomplete(value) {
      if (!(this instanceof OfflineAudioContext)) {
        throw new TypeError("Invalid Invocation: Value of 'this' must be of type 'OfflineAudioContext'");
      }

      if (isFunction(value) || value === null) {
        this._complete = value;
      }
    }

    async startRendering() {
      if (!(this instanceof OfflineAudioContext)) {
        throw new TypeError("Invalid Invocation: Value of 'this' must be of type 'OfflineAudioContext'");
      }

      // Lazily register event callback on rust side
      bridgeEventTarget(this);

      const nativeAudioBuffer = await this[kNapiObj].startRendering();
      const audioBuffer = new jsExport.AudioBuffer({ [kNativeAudioBuffer]: nativeAudioBuffer });

      // We dispatch the complete event manually to simplify the sharing of the
      // `AudioBuffer` instance. This also simplifies code on the rust side as
      // we don't need to deal with the `OfflineAudioCompletionEvent` type.
      const event = new Event('complete');
      event.renderedBuffer = audioBuffer;

      if (isFunction(this[`oncomplete`])) {
        this[`oncomplete`](event);
      }

      this.dispatchEvent(event);

      return audioBuffer;
    }

    async resume() {
      if (!(this instanceof OfflineAudioContext)) {
        throw new TypeError("Invalid Invocation: Value of 'this' must be of type 'OfflineAudioContext'");
      }

      await this[kNapiObj].resume();
    }

    async suspend(suspendTime) {
      if (!(this instanceof OfflineAudioContext)) {
        throw new TypeError("Invalid Invocation: Value of 'this' must be of type 'OfflineAudioContext'");
      }

      if (arguments.length < 1) {
        throw new TypeError(`Failed to execute 'suspend' on 'OfflineAudioContext': 1 argument required, but only ${arguments.length} present`);
      }

      await this[kNapiObj].suspend(suspendTime);
    }
  }

  Object.defineProperties(OfflineAudioContext, {
    length: {
      __proto__: null,
      writable: false,
      enumerable: false,
      configurable: true,
      value: 1,
    },
  });

  Object.defineProperties(OfflineAudioContext.prototype, {
    [Symbol.toStringTag]: {
      __proto__: null,
      writable: false,
      enumerable: false,
      configurable: true,
      value: 'OfflineAudioContext',
    },

    length: kEnumerableProperty,
    oncomplete: kEnumerableProperty,
    startRendering: kEnumerableProperty,
    resume: kEnumerableProperty,
    suspend: kEnumerableProperty,
  });

  return OfflineAudioContext;
};

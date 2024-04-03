// -------------------------------------------------------------------------- //
// -------------------------------------------------------------------------- //
//                                                                            //
//                                                                            //
//                                                                            //
//    ██╗    ██╗ █████╗ ██████╗ ███╗   ██╗██╗███╗   ██╗ ██████╗               //
//    ██║    ██║██╔══██╗██╔══██╗████╗  ██║██║████╗  ██║██╔════╝               //
//    ██║ █╗ ██║███████║██████╔╝██╔██╗ ██║██║██╔██╗ ██║██║  ███╗              //
//    ██║███╗██║██╔══██║██╔══██╗██║╚██╗██║██║██║╚██╗██║██║   ██║              //
//    ╚███╔███╔╝██║  ██║██║  ██║██║ ╚████║██║██║ ╚████║╚██████╔╝              //
//     ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝╚═╝  ╚═══╝ ╚═════╝               //
//                                                                            //
//                                                                            //
//    - This file has been generated ---------------------------              //
//                                                                            //
//                                                                            //
// -------------------------------------------------------------------------- //
// -------------------------------------------------------------------------- //

const {
  throwSanitizedError,
} = require('./lib/errors.js');

const {
  kEnumerableProperty,
} = require('./lib/utils.js');
const {
  kNativeAudioParam,
} = require('./lib/symbols.js');

class AudioParam {
  constructor(nativeAudioParam) {
    if (nativeAudioParam['Symbol.toStringTag'] !== 'AudioParam') {
      throw new TypeError('Illegal constructor');
    }

    this[kNativeAudioParam] = nativeAudioParam;
  }
  // getters

  get value() {
    return this[kNativeAudioParam].value;
  }

  get automationRate() {
    return this[kNativeAudioParam].automationRate;
  }

  get defaultValue() {
    return this[kNativeAudioParam].defaultValue;
  }

  get minValue() {
    return this[kNativeAudioParam].minValue;
  }

  get maxValue() {
    return this[kNativeAudioParam].maxValue;
  }

  // setters

  set value(value) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    try {
      this[kNativeAudioParam].value = value;
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  set automationRate(value) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    try {
      this[kNativeAudioParam].automationRate = value;
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  // methods

  setValueAtTime(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 2) {
      throw new TypeError(`Failed to execute 'setValueAtTime' on 'AudioParam': 2 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].setValueAtTime(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  linearRampToValueAtTime(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 2) {
      throw new TypeError(`Failed to execute 'linearRampToValueAtTime' on 'AudioParam': 2 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].linearRampToValueAtTime(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  exponentialRampToValueAtTime(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 2) {
      throw new TypeError(`Failed to execute 'exponentialRampToValueAtTime' on 'AudioParam': 2 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].exponentialRampToValueAtTime(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  setTargetAtTime(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 3) {
      throw new TypeError(`Failed to execute 'setTargetAtTime' on 'AudioParam': 3 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].setTargetAtTime(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  setValueCurveAtTime(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 3) {
      throw new TypeError(`Failed to execute 'setValueCurveAtTime' on 'AudioParam': 3 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].setValueCurveAtTime(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  cancelScheduledValues(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 1) {
      throw new TypeError(`Failed to execute 'cancelScheduledValues' on 'AudioParam': 1 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].cancelScheduledValues(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

  cancelAndHoldAtTime(...args) {
    if (!(this instanceof AudioParam)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioParam\'');
    }

    if (arguments.length < 1) {
      throw new TypeError(`Failed to execute 'cancelAndHoldAtTime' on 'AudioParam': 1 argument required, but only ${arguments.length} present`);
    }

    try {
      return this[kNativeAudioParam].cancelAndHoldAtTime(...args);
    } catch (err) {
      throwSanitizedError(err);
    }
  }

}

Object.defineProperties(AudioParam, {
  length: {
    __proto__: null,
    writable: false,
    enumerable: false,
    configurable: true,
    value: 0,
  },
});

Object.defineProperties(AudioParam.prototype, {
  [Symbol.toStringTag]: {
    __proto__: null,
    writable: false,
    enumerable: false,
    configurable: true,
    value: 'AudioParam',
  },

  value: kEnumerableProperty,
  automationRate: kEnumerableProperty,
  defaultValue: kEnumerableProperty,
  minValue: kEnumerableProperty,
  maxValue: kEnumerableProperty,

  setValueAtTime: kEnumerableProperty,
  linearRampToValueAtTime: kEnumerableProperty,
  exponentialRampToValueAtTime: kEnumerableProperty,
  setTargetAtTime: kEnumerableProperty,
  setValueCurveAtTime: kEnumerableProperty,
  cancelScheduledValues: kEnumerableProperty,
  cancelAndHoldAtTime: kEnumerableProperty,

});

module.exports = AudioParam;

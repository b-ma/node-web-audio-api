const {
  parentPort,
} = require('node:worker_threads');

const {
  register_params,
  run_audio_worklet,
} = require('../index.cjs');

const kMessagePort = Symbol('node-web-audio-api:message-port');
const nameProcessorCtorMap = new Map();
const processorIdMap = new WeakMap();
let loopStarted = false;
let breakLoop = false;

function runLoop() {
  // block until we need to render a quantum
  run_audio_worklet();

  if (!breakLoop) {
    // yield to the event loop, and then repeat
    setImmediate(runLoop);
  }
}

class AudioWorkletProcessor {
  #port = null;

  constructor(options) {
    this.#port = options[kMessagePort];
  }

  get port() {
    if (!(this instanceof AudioWorkletProcessor)) {
      throw new TypeError('Invalid Invocation: Value of \'this\' must be of type \'AudioWorkletProcessor\'');
    }

    return this.#port;
  }
}

function registerProcessor(name, processorCtor) {
  nameProcessorCtorMap.set(name, processorCtor);
  // send back to main thread and resolve Promise
  console.log('> registered processor', name, processorCtor.parameterDescriptors);
}

// NOTE: Authors that register an event listener on the "message" event of this
// port should call close on either end of the MessageChannel (either in the
// AudioWorklet or the AudioWorkletGlobalScope side) to allow for resources to be collected.
parentPort.on('close', () => {
  breakLoop = true;
  // @todo
  // - clear all maps
  // - etc...
  return;
});

parentPort.on('message', event => {
  process.stdout.write(event.cmd + '\n');

  switch (event.cmd) {
    case 'node-web-audio-api:worklet:add-module': {
      const { code } = event;
      const func = new Function('AudioWorkletProcessor', 'registerProcessor', code);
      func(AudioWorkletProcessor, registerProcessor);

      // @todo - send back parameterDescriptors
      break;
    }
    case 'node-web-audio-api:worklet:create-processor': {
      const { name, processorOptions, messagePort, id } = event;
      const ctor = nameProcessorCtorMap.get(name);

      register_params(ctor.parameterDescriptors ?? []);

      processorOptions[kMessagePort] = messagePort;
      const instance = new ctor(processorOptions);

      globalThis.processorIdMap.set(id, instance);

      if (!loopStarted) {
        loopStarted = true;
        runLoop();
      }
      break;
    }
  }
});

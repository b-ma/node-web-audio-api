use crate::{NapiAudioContext, NapiAudioParam, NapiOfflineAudioContext};

use crossbeam_channel::{self, Receiver, Sender};

use napi::*;
use napi_derive::js_function;

use web_audio_api::context::{BaseAudioContext, ConcreteBaseAudioContext};
use web_audio_api::node::{AudioNode, AudioNodeOptions, ChannelCountMode, ChannelInterpretation};
use web_audio_api::worklet::{
    AudioParamValues, AudioWorkletGlobalScope, AudioWorkletNode, AudioWorkletNodeOptions,
    AudioWorkletProcessor,
};
use web_audio_api::{AudioParamDescriptor, AutomationRate};

use std::cell::Cell;
use std::collections::HashMap;
use std::option::Option;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};

use std::time::{Duration, Instant};

/// Unique ID generator for AudioWorkletProcessors
static INCREMENTING_ID: AtomicU32 = AtomicU32::new(0);

/// Command issued from render thread to the Worker
#[derive(Debug)]
enum WorkletCommand {
    Drop(u32),
    Process(ProcessorArguments),
    Tick,
}

/// Render thread to Worker processor arguments
#[derive(Debug)]
struct ProcessorArguments {
    // processor unique ID
    id: u32,
    // processor inputs (unsafely cast to static)
    inputs: &'static [&'static [&'static [f32]]],
    // processor ouputs (unsafely cast to static)
    outputs: &'static [&'static [&'static [f32]]],
    // processor audio params (unsafely cast to static)
    param_values: &'static [(&'static str, &'static [f32])],
    // AudioWorkletGlobalScope currentTime
    current_time: f64,
    // AudioWorkletGlobalScope currentFrame
    current_frame: u64,
    // channel for tail_time return value
    tail_time_sender: Sender<bool>,
}

/// Message channel from render thread to Worker
struct ProcessCallChannel {
    // queue of worklet commands
    command_buffer: Mutex<Vec<WorkletCommand>>,
    // Condition Variable to wait/notify on new worklet commands
    cond_var: Condvar,
    // mark that the worklet has been exited to prevent any further `process` call
    exited: AtomicBool,
}

impl ProcessCallChannel {
    fn push(&self, command: WorkletCommand) {
        let mut buffer = self.command_buffer.lock().unwrap();
        buffer.push(command);
        self.cond_var.notify_one();
    }

    fn pop(&self) -> WorkletCommand {
        let mut buffer = self.command_buffer.lock().unwrap();
        while buffer.is_empty() {
            buffer = self.cond_var.wait(buffer).unwrap();
        }
        buffer.remove(0)
    }

    fn try_pop(&self) -> Option<WorkletCommand> {
        let mut buffer = self.command_buffer.lock().unwrap();

        if buffer.is_empty() {
            return None;
        }

        Some(buffer.remove(0))
    }
}

/// Global map of ID -> ProcessCallChannel
///
/// Every (Offline)AudioContext is assigned a new channel + ID. The ID is passed to the
/// AudioWorklet Worker and to every AudioNode in the context so they can grab the channel and use
/// message passing.
static GLOBAL_PROCESS_CALL_CHANNEL_MAP: RwLock<Vec<Arc<ProcessCallChannel>>> = RwLock::new(vec![]);

/// Request a new channel + ID for a newly created (Offline)AudioContext
pub(crate) fn allocate_process_call_channel(ctx: &ConcreteBaseAudioContext) -> usize {
    // Only one process message can be sent at same time from a given context,
    // but Drop messages could be send too, so let's take some room
    let command_buffer = Mutex::new(Vec::with_capacity(32));

    let channel = ProcessCallChannel {
        command_buffer,
        cond_var: Condvar::new(),
        exited: AtomicBool::new(false),
    };
    let channel = Arc::new(channel);

    let options = AudioWorkletNodeOptions {
        number_of_inputs: 1,
        number_of_outputs: 1, // should be zero, but bug in base lib
        output_channel_count: Default::default(),
        parameter_data: Default::default(),
        audio_node_options: AudioNodeOptions::default(),
        processor_options: channel.clone(),
    };
    let node = AudioWorkletNode::new::<RenderTickProcessor>(ctx, options);
    ctx.destination().connect(&node);

    // We need a write-lock to initialize the channel
    let mut write_lock = GLOBAL_PROCESS_CALL_CHANNEL_MAP.write().unwrap();
    let id = write_lock.len();
    write_lock.push(channel);

    id
}

/// Obtain the WorkletCommand sender for this context ID
fn process_call_channel(id: usize) -> Arc<ProcessCallChannel> {
    // optimistically assume the channel exists and we can use a shared read-lock
    Arc::clone(&GLOBAL_PROCESS_CALL_CHANNEL_MAP.read().unwrap()[id])
}

/// Message channel inside the control thread to pass param descriptors of a given AudioWorkletNode
/// into the static method AudioWorkletProcessor::parameter_descriptors
struct AudioParamDescriptorsChannel {
    send: Mutex<Sender<Vec<AudioParamDescriptor>>>,
    recv: Receiver<Vec<AudioParamDescriptor>>,
}

/// Generate the AudioParamDescriptorsChannel
///
/// It is shared by the whole application, so even by different AudioContexts. This is no issue
/// because it's using a Mutex to prevent concurrency.
fn audio_param_descriptor_channel() -> &'static AudioParamDescriptorsChannel {
    static PAIR: OnceLock<AudioParamDescriptorsChannel> = OnceLock::new();
    PAIR.get_or_init(|| {
        let (send, recv) = crossbeam_channel::unbounded();
        AudioParamDescriptorsChannel {
            send: Mutex::new(send),
            recv,
        }
    })
}

thread_local! {
    /// Denotes if the Worker thread priority has already been upped
    static HAS_THREAD_PRIO: Cell<bool> = const { Cell::new(false) };
}

struct WorkletAbruptCompletionResult {
    cmd: String,
    err: Error,
}

/// Check that given JS and Rust input / output layout are the same, i.e. check
/// that each input / output have the same number of channels
///
/// Note that we don't check the number of inputs / outputs as they is defined
/// at construction and cannot change
fn check_same_io_layout(js_io: &JsObject, rs_io: &'static [&'static [&'static [f32]]]) -> bool {
    for (i, io) in rs_io.iter().enumerate() {
        if io.len()
            != js_io
                .get_element::<JsObject>(i as u32)
                .unwrap()
                .get_array_length_unchecked()
                .unwrap() as usize
        {
            return false;
        }
    }

    true
}

/// Recreate the whole JS inputs and output data structure. It is required to start
/// from scratch because Array are frozen with prevents us to add, remove or modify items.
fn rebuild_io_layout(
    env: &Env,
    js_io: JsObject,
    rs_io: &'static [&'static [&'static [f32]]],
    render_quantum_size: usize,
) -> JsObject {
    let mut new_js_io = env.create_array(rs_io.len() as u32).unwrap();

    for (i, io) in rs_io.iter().enumerate() {
        let mut channels = env.create_array(rs_io[i].len() as u32).unwrap();
        let old_channels = js_io.get_element::<JsObject>(i as u32).unwrap();

        for j in 0..io.len() {
            // Try to reuse existing Float32Array
            let float32_arr = if old_channels.has_element(j as u32).unwrap() {
                old_channels.get_element::<JsTypedArray>(j as u32).unwrap()
            } else {
                env.create_arraybuffer(render_quantum_size * 4)
                    .unwrap()
                    .into_raw()
                    .into_typedarray(napi::TypedArrayType::Float32, render_quantum_size, 0)
                    .unwrap()
            };

            let _ = channels.set(j as u32, float32_arr);
        }

        let mut channels = channels.coerce_to_object().unwrap();
        let _ = channels.freeze();

        new_js_io.set(i as u32, channels).unwrap();
    }

    let mut new_js_io = new_js_io.coerce_to_object().unwrap();
    let _ = new_js_io.freeze();

    new_js_io
}

/// Handle a AudioWorkletProcessor::process call in the Worker
fn process_audio_worklet(env: &Env, processors: &JsObject, args: ProcessorArguments) -> Result<()> {
    let ProcessorArguments {
        id,
        inputs,
        outputs,
        param_values,
        current_time,
        current_frame,
        tail_time_sender,
    } = args;

    let processor = processors.get_named_property::<JsUnknown>(&id.to_string())?;

    // Make sure the processor exists, might run into race conditions
    // between Rust Audio thread and JS Worker thread
    if processor.get_type()? == ValueType::Undefined {
        let _ = tail_time_sender.send(true); // make sure we will be called
        return Ok(());
    }

    // fill AudioWorkletGlobalScope
    let mut global = env.get_global()?;
    global.set_named_property("currentTime", current_time)?;
    global.set_named_property("currentFrame", current_frame)?;

    let mut processor = processor.coerce_to_object()?;

    let k_worklet_callable_process =
        env.symbol_for("node-web-audio-api:worklet-callable-process")?;
    // return early if worklet has been tagged as not callable,
    // @note - maybe this could be guaranteed on rust side
    let callable_process = processor
        .get_property::<JsSymbol, JsBoolean>(k_worklet_callable_process)?
        .get_value()?;

    if !callable_process {
        let _ = tail_time_sender.send(false);
        return Ok(());
    }

    // This value become Some if "process" do not exist or throw an error at execution
    let mut completion: Option<WorkletAbruptCompletionResult> = None;

    match processor.get_named_property::<JsFunction>("process") {
        Ok(process_method) => {
            let k_worklet_inputs = env.symbol_for("node-web-audio-api:worklet-inputs")?;
            let k_worklet_outputs = env.symbol_for("node-web-audio-api:worklet-outputs")?;
            let k_worklet_params = env.symbol_for("node-web-audio-api:worklet-params")?;
            let k_worklet_params_cache =
                env.symbol_for("node-web-audio-api:worklet-params-cache")?;

            // @todo - get from global
            let render_quantum_size = 128;
            let mut js_inputs = processor.get_property::<JsSymbol, JsObject>(k_worklet_inputs)?;
            let mut js_outputs = processor.get_property::<JsSymbol, JsObject>(k_worklet_outputs)?;
            let mut js_params = processor.get_property::<JsSymbol, JsObject>(k_worklet_params)?;
            let js_params_cache =
                processor.get_property::<JsSymbol, JsObject>(k_worklet_params_cache)?;

            // Check JS input and output, and rebuild JS object if layout changed
            if !check_same_io_layout(&js_inputs, inputs) {
                let new_js_inputs = rebuild_io_layout(env, js_inputs, inputs, render_quantum_size);
                // Store new layout in processor
                processor.set_property(k_worklet_inputs, new_js_inputs)?;
                // Override js_inputs with new reference
                js_inputs = processor.get_property::<JsSymbol, JsObject>(k_worklet_inputs)?;
            }

            if !check_same_io_layout(&js_outputs, outputs) {
                let new_js_outputs =
                    rebuild_io_layout(env, js_outputs, outputs, render_quantum_size);
                // Store new layout in processor
                processor.set_property(k_worklet_outputs, new_js_outputs)?;
                // Override js_outputs with new reference
                js_outputs = processor.get_property::<JsSymbol, JsObject>(k_worklet_outputs)?;
            }

            // Copy inputs into JS inputs buffers
            for (input_number, input) in inputs.iter().enumerate() {
                let js_input = js_inputs.get_element::<JsObject>(input_number as u32)?;

                for (channel_number, channel) in input.iter().enumerate() {
                    let js_channel = js_input.get_element::<JsTypedArray>(channel_number as u32)?;
                    let mut js_channel_value = js_channel.into_value()?;
                    let js_channel_buffer: &mut [f32] = js_channel_value.as_mut();
                    js_channel_buffer.copy_from_slice(channel);
                }
            }

            // Copy params values into JS params buffers
            //
            // @perf - We could rely on the fact that ParameterDescriptors
            // are ordered maps to avoid sending param names in `param_values`
            for (name, data) in param_values.iter() {
                let float32_arr_cache = js_params_cache.get_named_property::<JsObject>(name)?;
                // retrieve right Float32Array according to actual param size, i.e. 128 or 1
                let cache_index = if data.len() == 1 { 1 } else { 0 };
                let float32_arr = float32_arr_cache.get_element::<JsTypedArray>(cache_index)?;
                // copy data into underlying ArrayBuffer
                let mut float32_arr_value = float32_arr.into_value()?;
                let buffer: &mut [f32] = float32_arr_value.as_mut();
                buffer.copy_from_slice(data);
                // get new owned value, as `float32_arr` as been consumed by `into_value` call
                let float32_arr = float32_arr_cache.get_element::<JsTypedArray>(cache_index)?;
                js_params.set_named_property(name, float32_arr)?;
            }

            let res: Result<JsUnknown> =
                process_method.apply3(processor, js_inputs, js_outputs, js_params);

            match res {
                Ok(js_ret) => {
                    // Grab back new owned value processor and js_ouputs, has been
                    // consumed by `apply` call
                    let processor = processors.get_named_property::<JsObject>(&id.to_string())?;
                    let js_outputs =
                        processor.get_property::<JsSymbol, JsObject>(k_worklet_outputs)?;

                    // copy JS output buffers back into outputs
                    for (output_number, output) in outputs.iter().enumerate() {
                        let js_output = js_outputs.get_element::<JsObject>(output_number as u32)?;

                        for (channel_number, channel) in output.iter().enumerate() {
                            let js_channel =
                                js_output.get_element::<JsTypedArray>(channel_number as u32)?;
                            let js_channel_value = js_channel.into_value()?;
                            let js_channel_buffer: &[f32] = js_channel_value.as_ref();

                            let src = js_channel_buffer.as_ptr();
                            let dst = channel.as_ptr() as *mut f32;

                            unsafe {
                                std::ptr::copy_nonoverlapping(src, dst, render_quantum_size);
                            }
                        }
                    }

                    let ret = js_ret.coerce_to_bool()?.get_value()?;
                    let _ = tail_time_sender.send(ret); // allowed to fail
                }
                Err(err) => {
                    completion = Some(WorkletAbruptCompletionResult {
                        cmd: "node-web-audio-api:worklet:process-error".to_string(),
                        err,
                    });
                }
            }
        }
        Err(err) => {
            completion = Some(WorkletAbruptCompletionResult {
                cmd: "node-web-audio-api:worklet:process-invalid".to_string(),
                err,
            });
        }
    }

    // Handle eventual errors
    if let Some(value) = completion {
        let WorkletAbruptCompletionResult { cmd, err } = value;
        // Grab back our process which may have been consumed by the process apply
        let mut processor = global.get_named_property::<JsObject>(&id.to_string())?;
        let k_worklet_queue_task = env.symbol_for("node-web-audio-api:worklet-queue-task")?;
        // @todo - would be usefull to propagate to rust side too so that the
        // processor can be removed from graph (?)
        let value = env.get_boolean(false)?;
        processor.set_property(k_worklet_callable_process, value)?;
        // set active source flag to false, same semantic as tail time
        // https://webaudio.github.io/web-audio-api/#active-source
        let _ = tail_time_sender.send(false);
        // Dispatch processorerror event on main thread
        let queue_task = processor.get_property::<JsSymbol, JsFunction>(k_worklet_queue_task)?;
        let js_cmd = env.create_string(&cmd)?;
        let js_err = env.create_error(err)?;
        queue_task.apply2(processor, js_cmd, js_err)?;
    }

    Ok(())
}

static PREV_START: RwLock<Option<Instant>> = RwLock::new(None);

/// The entry point into Rust from the Worker
#[js_function(2)]
pub(crate) fn run_audio_worklet_global_scope(ctx: CallContext) -> Result<JsUndefined> {
    let enter_start = Instant::now();
    let mut lock = PREV_START.write().unwrap();
    if let Some(prev) = *lock {
        let micros = enter_start.duration_since(prev).as_micros();
        if micros > 200 {
            println!("return to Rust after {} micros", micros);
        }
    }

    // Set thread priority to highest, if not done already
    if !HAS_THREAD_PRIO.replace(true) {
        let result = audio_thread_priority::promote_current_thread_to_real_time(
            128, 44100, // TODO get sample rate
        );
        dbg!(&result);
        result.ok(); // allowed to fail
    }

    // Obtain the unique worker ID
    let worklet_id = ctx.get::<JsNumber>(0)?.get_uint32()? as usize;
    // List of registered processors
    let processors = ctx.get::<JsObject>(1)?;

    // Poll for incoming commands and yield back to the event loop if there are none.
    // recv_timeout is not an option due to realtime safety, see discussion of
    // https://github.com/ircam-ismm/node-web-audio-api/pull/124#pullrequestreview-2053515583
    let mut prev = Instant::now();
    loop {
        let cmd = process_call_channel(worklet_id).pop();
        let now = Instant::now();
        let micros = now.duration_since(prev).as_micros();
        if micros > 3000 {
            println!("got command after {} micros", micros);
        }

        match cmd {
            WorkletCommand::Drop(id) => {
                let mut processors = ctx.get::<JsObject>(1)?;
                processors.delete_named_property(&id.to_string()).unwrap();
            }
            WorkletCommand::Process(args) => {
                process_audio_worklet(ctx.env, &processors, args)?;
            }
            WorkletCommand::Tick => {
                break;
            }
        }

        let end = Instant::now();
        let micros = end.duration_since(now).as_micros();
        if micros > 200 {
            println!("handled command after {} micros", micros);
        }

        prev = now;
    }

    *lock = Some(Instant::now());
    ctx.env.get_undefined()
}

#[js_function(1)]
pub(crate) fn exit_audio_worklet_global_scope(ctx: CallContext) -> Result<JsUndefined> {
    // Obtain the unique worker ID
    let worklet_id = ctx.get::<JsNumber>(0)?.get_uint32()? as usize;
    // Flag message channel as exited to prevent any other render call
    process_call_channel(worklet_id)
        .exited
        .store(true, Ordering::SeqCst);
    // Handle any pending message from audio thread
    if let Some(WorkletCommand::Process(args)) = process_call_channel(worklet_id).try_pop() {
        let _ = args.tail_time_sender.send(false);
    }

    ctx.env.get_undefined()
}

pub(crate) struct NapiAudioWorkletNode(AudioWorkletNode);

impl NapiAudioWorkletNode {
    pub fn create_js_class(env: &Env) -> Result<JsFunction> {
        let interface = audio_node_interface![];

        env.define_class("AudioWorkletNode", constructor, &interface)
    }

    pub fn unwrap(&self) -> &AudioWorkletNode {
        &self.0
    }
}

#[js_function(4)]
fn constructor(ctx: CallContext) -> Result<JsUndefined> {
    let mut js_this = ctx.this_unchecked::<JsObject>();

    let js_audio_context = ctx.get::<JsObject>(0)?;

    // @note - not used, handled in the JS code
    // let js_name = ctx.get::<JsString>(1)?;

    // --------------------------------------------------------
    // Parse options
    // --------------------------------------------------------
    let options_js = ctx.get::<JsObject>(2)?;

    let number_of_inputs = options_js
        .get_named_property::<JsNumber>("numberOfInputs")?
        .get_double()? as usize;

    let number_of_outputs = options_js
        .get_named_property::<JsNumber>("numberOfOutputs")?
        .get_double()? as usize;

    let output_channel_count_js = options_js
        .get::<&str, JsTypedArray>("outputChannelCount")?
        .unwrap();
    let output_channel_count_value = output_channel_count_js.into_value()?;
    let output_channel_count_u32: &[u32] = output_channel_count_value.as_ref();
    let output_channel_count: Vec<usize> = output_channel_count_u32
        .iter()
        .map(|&v| v as usize)
        .collect();

    let mut parameter_data = HashMap::<String, f64>::new();
    let parameter_data_js = options_js.get_named_property::<JsObject>("parameterData")?;
    let parameter_keys_js = parameter_data_js.get_all_property_names(
        KeyCollectionMode::OwnOnly,
        KeyFilter::Enumerable,
        KeyConversion::NumbersToStrings,
    )?;
    let length = parameter_keys_js.get_array_length()?;

    for i in 0..length {
        let key_js = parameter_keys_js.get_element::<JsString>(i)?;
        let utf8_key = key_js.into_utf8()?;
        let key = utf8_key.into_owned()?;

        let value = parameter_data_js
            .get_property::<JsString, JsNumber>(key_js)?
            .get_double()?;

        parameter_data.insert(key, value);
    }

    // No `processorOptions` here, they are sent to JS processor

    // --------------------------------------------------------
    // Parse AudioNodeOptions
    // --------------------------------------------------------
    let audio_node_options_default = AudioNodeOptions::default();

    let some_channel_count_js = options_js.get::<&str, JsObject>("channelCount")?;
    let channel_count = if let Some(channel_count_js) = some_channel_count_js {
        channel_count_js.coerce_to_number()?.get_double()? as usize
    } else {
        audio_node_options_default.channel_count
    };

    let some_channel_count_mode_js = options_js.get::<&str, JsObject>("channelCountMode")?;
    let channel_count_mode = if let Some(channel_count_mode_js) = some_channel_count_mode_js {
        let channel_count_mode_str = channel_count_mode_js
            .coerce_to_string()?
            .into_utf8()?
            .into_owned()?;

        match channel_count_mode_str.as_str() {
            "max" => ChannelCountMode::Max,
            "clamped-max" => ChannelCountMode::ClampedMax,
            "explicit" => ChannelCountMode::Explicit,
            _ => unreachable!(),
        }
    } else {
        audio_node_options_default.channel_count_mode
    };

    let some_channel_interpretation_js =
        options_js.get::<&str, JsObject>("channelInterpretation")?;
    let channel_interpretation =
        if let Some(channel_interpretation_js) = some_channel_interpretation_js {
            let channel_interpretation_str = channel_interpretation_js
                .coerce_to_string()?
                .into_utf8()?
                .into_owned()?;

            match channel_interpretation_str.as_str() {
                "speakers" => ChannelInterpretation::Speakers,
                "discrete" => ChannelInterpretation::Discrete,
                _ => unreachable!(),
            }
        } else {
            audio_node_options_default.channel_interpretation
        };

    // --------------------------------------------------------
    // Parse ParameterDescriptors
    // --------------------------------------------------------
    let params_js = ctx.get::<JsObject>(3)?;
    let length = params_js.get_array_length()? as usize;
    let mut rs_params: Vec<web_audio_api::AudioParamDescriptor> = Vec::with_capacity(length);

    for i in 0..length {
        let param = params_js.get_element::<JsObject>(i.try_into().unwrap())?;

        let js_name = param.get_named_property::<JsString>("name").unwrap();
        let utf8_name = js_name.into_utf8().unwrap();
        let name = utf8_name.into_owned().unwrap();

        let min_value = param
            .get_named_property::<JsNumber>("minValue")
            .unwrap()
            .get_double()
            .unwrap() as f32;

        let max_value = param
            .get_named_property::<JsNumber>("maxValue")
            .unwrap()
            .get_double()
            .unwrap() as f32;

        let default_value = param
            .get_named_property::<JsNumber>("defaultValue")
            .unwrap()
            .get_double()
            .unwrap() as f32;

        let js_str = param.get_named_property::<JsString>("automationRate")?;
        let utf8_str = js_str.coerce_to_string()?.into_utf8()?.into_owned()?;
        let automation_rate = match utf8_str.as_str() {
            "a-rate" => AutomationRate::A,
            "k-rate" => AutomationRate::K,
            _ => unreachable!(),
        };

        let param_descriptor = AudioParamDescriptor {
            name,
            min_value,
            max_value,
            default_value,
            automation_rate,
        };

        rs_params.insert(i, param_descriptor);
    }

    let audio_context_name =
        js_audio_context.get_named_property::<JsString>("Symbol.toStringTag")?;
    let audio_context_str = audio_context_name.into_utf8()?;

    let worklet_id = match audio_context_str.as_str()? {
        "AudioContext" => {
            let napi_audio_context = ctx.env.unwrap::<NapiAudioContext>(&js_audio_context)?;
            napi_audio_context.worklet_id()
        }
        "OfflineAudioContext" => {
            let napi_audio_context = ctx
                .env
                .unwrap::<NapiOfflineAudioContext>(&js_audio_context)?;
            napi_audio_context.worklet_id()
        }
        &_ => panic!("not supported"),
    };

    // --------------------------------------------------------
    // Create AudioWorkletNodeOptions object
    // --------------------------------------------------------
    let id = INCREMENTING_ID.fetch_add(1, Ordering::Relaxed);
    let processor_options = NapiAudioWorkletProcessor {
        id,
        command_channel: process_call_channel(worklet_id),
        tail_time_channel: crossbeam_channel::bounded(1),
        param_values: Vec::with_capacity(32),
    };

    let options = AudioWorkletNodeOptions {
        number_of_inputs,
        number_of_outputs,
        output_channel_count,
        parameter_data,
        audio_node_options: AudioNodeOptions {
            channel_count,
            channel_count_mode,
            channel_interpretation,
        },
        processor_options,
    };

    // --------------------------------------------------------
    // send parameterDescriptors so that NapiAudioWorkletProcessor can retrieve them
    // --------------------------------------------------------
    let guard = audio_param_descriptor_channel().send.lock().unwrap();
    guard.send(rs_params).unwrap();

    // --------------------------------------------------------
    // Create native AudioWorkletNode
    // --------------------------------------------------------
    let native_node = match audio_context_str.as_str()? {
        "AudioContext" => {
            let napi_audio_context = ctx.env.unwrap::<NapiAudioContext>(&js_audio_context)?;
            let audio_context = napi_audio_context.unwrap();
            AudioWorkletNode::new::<NapiAudioWorkletProcessor>(audio_context, options)
        }
        "OfflineAudioContext" => {
            let napi_audio_context = ctx
                .env
                .unwrap::<NapiOfflineAudioContext>(&js_audio_context)?;
            let audio_context = napi_audio_context.unwrap();
            AudioWorkletNode::new::<NapiAudioWorkletProcessor>(audio_context, options)
        }
        &_ => unreachable!(),
    };

    drop(guard);

    let mut js_parameters = ctx.env.create_object()?;

    for (name, native_param) in native_node.parameters().iter() {
        let native_param = native_param.clone();
        let napi_param = NapiAudioParam::new(native_param);
        let mut js_obj = NapiAudioParam::create_js_object(ctx.env)?;
        ctx.env.wrap(&mut js_obj, napi_param)?;

        js_parameters.set_named_property(name, js_obj)?;
    }

    // --------------------------------------------------------
    // Finalize instance creation
    // --------------------------------------------------------
    js_this.define_properties(&[
        Property::new("context")?
            .with_value(&js_audio_context)
            .with_property_attributes(PropertyAttributes::Enumerable),
        Property::new("parameters")?
            .with_value(&js_parameters)
            .with_property_attributes(PropertyAttributes::Enumerable),
        Property::new("id")?
            .with_value(&ctx.env.create_uint32(id)?)
            .with_property_attributes(PropertyAttributes::Enumerable),
        // this must be put on the instance and not in the prototype to be reachable
        Property::new("Symbol.toStringTag")?
            .with_value(&ctx.env.create_string("AudioWorkletNode")?)
            .with_property_attributes(PropertyAttributes::Static),
    ])?;

    // finalize instance creation
    let napi_node = NapiAudioWorkletNode(native_node);
    ctx.env.wrap(&mut js_this, napi_node)?;

    ctx.env.get_undefined()
}

audio_node_impl!(NapiAudioWorkletNode);

// -------------------------------------------------
// AudioWorkletNode Interface
// -------------------------------------------------

struct NapiAudioWorkletProcessor {
    /// Unique id to pair Napi Worklet and JS processor
    id: u32,
    /// Command channel to the JS Worklet
    command_channel: Arc<ProcessCallChannel>,
    /// tail_time result channel
    tail_time_channel: (Sender<bool>, Receiver<bool>),
    /// Reusable Vec for AudioParam values
    param_values: Vec<(&'static str, &'static [f32])>,
}

impl AudioWorkletProcessor for NapiAudioWorkletProcessor {
    type ProcessorOptions = NapiAudioWorkletProcessor;

    fn constructor(opts: Self::ProcessorOptions) -> Self {
        opts // the opts contain the full processor
    }

    fn parameter_descriptors() -> Vec<AudioParamDescriptor>
    where
        Self: Sized,
    {
        // Get the values out of thin air, see `audio_param_descriptor_channel()` for details
        audio_param_descriptor_channel().recv.recv().unwrap()
    }

    fn process<'a, 'b>(
        &mut self,
        inputs: &'b [&'a [&'a [f32]]],
        outputs: &'b mut [&'a mut [&'a mut [f32]]],
        params: AudioParamValues<'b>,
        scope: &'b AudioWorkletGlobalScope,
    ) -> bool {
        // Early return if audio thread is still closing while worklet has been exited
        if self.command_channel.exited.load(Ordering::SeqCst) {
            return false;
        }

        // SAFETY:
        // We are transmuting the a' and b' lifetimes to static in order to send them to the Worker
        // thread. This should be safe as long as:
        // - this function does not return before the Worker has finished using the slices
        // - the Worker / JS-code doesn't keep a copy of these slices - fingers crossed on this one

        let inputs: &'static [&'static [&'static [f32]]] = unsafe { std::mem::transmute(inputs) };
        let outputs: &'static [&'static [&'static [f32]]] = unsafe { std::mem::transmute(outputs) };

        self.param_values.clear();
        self.param_values.extend(params.keys().map(|k| {
            let label: &'static str = unsafe { std::mem::transmute(k) };
            let value: &'static [f32] = unsafe { std::mem::transmute(&params.get(k)[..]) };
            (label, value)
        }));
        let param_values: &'static [_] = unsafe { std::mem::transmute(&self.param_values[..]) };

        // end SAFETY comment

        let item = ProcessorArguments {
            id: self.id,
            inputs,
            outputs,
            param_values,
            current_time: scope.current_time,
            current_frame: scope.current_frame,
            tail_time_sender: self.tail_time_channel.0.clone(),
        };

        // send command to Worker
        self.command_channel.push(WorkletCommand::Process(item));
        // await result
        self.tail_time_channel.1.recv().unwrap()
    }
}

impl Drop for NapiAudioWorkletProcessor {
    fn drop(&mut self) {
        if !self.command_channel.exited.load(Ordering::SeqCst) {
            self.command_channel.push(WorkletCommand::Drop(self.id));
        }
    }
}

struct RenderTickProcessor {
    channel: Arc<ProcessCallChannel>,
}

impl AudioWorkletProcessor for RenderTickProcessor {
    type ProcessorOptions = Arc<ProcessCallChannel>;

    fn constructor(channel: Self::ProcessorOptions) -> Self {
        Self { channel }
    }

    fn process<'a, 'b>(
        &mut self,
        _inputs: &'b [&'a [&'a [f32]]],
        _outputs: &'b mut [&'a mut [&'a mut [f32]]],
        _params: AudioParamValues<'b>,
        _scope: &'b AudioWorkletGlobalScope,
    ) -> bool {
        self.channel.push(WorkletCommand::Tick);
        true
    }
}

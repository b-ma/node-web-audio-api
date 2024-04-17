#![deny(clippy::all)]

use napi::{Env, JsObject, Result};
use napi_derive::module_exports;

mod utils;

#[macro_use]
mod base_audio_context;
#[macro_use]
mod audio_node;

// Web Audio API
mod audio_context;
use crate::audio_context::NapiAudioContext;
mod audio_destination_node;
use crate::audio_destination_node::NapiAudioDestinationNode;
mod audio_param;
use crate::audio_param::NapiAudioParam;
mod audio_listener;
use crate::audio_listener::NapiAudioListener;
mod audio_buffer;
use crate::audio_buffer::NapiAudioBuffer;
mod periodic_wave;
use crate::periodic_wave::NapiPeriodicWave;
mod offline_audio_context;
use crate::offline_audio_context::NapiOfflineAudioContext;
// Generated audio nodes
${d.nodes.map(n => { return `
mod ${d.slug(n)};
use crate::${d.slug(n)}::${d.napiName(n)};`}).join('')}

// MediaDevices & MediaStream API
mod media_streams;
use crate::media_streams::NapiMediaStream;
mod media_devices;
use crate::media_devices::napi_enumerate_devices;
use crate::media_devices::napi_get_user_media;

#[cfg(all(
    any(windows, unix),
    target_arch = "x86_64",
    not(target_env = "musl"),
    not(debug_assertions)
))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[module_exports]
fn init(mut exports: JsObject, env: Env) -> Result<()> {
    // Do not print panic messages, handle through JS errors
    // std::panic::set_hook(Box::new(|_panic_info| {}));

    let napi_class = NapiAudioContext::create_js_class(&env)?;
    exports.set_named_property("AudioContext", napi_class)?;

    let napi_class = NapiOfflineAudioContext::create_js_class(&env)?;
    exports.set_named_property("OfflineAudioContext", napi_class)?;

    let napi_class = NapiAudioBuffer::create_js_class(&env)?;
    exports.set_named_property("AudioBuffer", napi_class)?;

    let napi_class = NapiPeriodicWave::create_js_class(&env)?;
    exports.set_named_property("PeriodicWave", napi_class)?;

    let napi_class = NapiMediaStreamAudioSourceNode::create_js_class(&env)?;
    exports.set_named_property("MediaStreamAudioSourceNode", napi_class)?;

    // ----------------------------------------------------------------
    // Generated audio nodes
    // ----------------------------------------------------------------
    ${d.nodes.map(n => { return `
    let napi_class = ${d.napiName(n)}::create_js_class(&env)?;
    exports.set_named_property("${d.name(n)}", napi_class)?;
    `}).join('')}

    // ----------------------------------------------------------------
    // MediaStream API & Media Devices API
    // ----------------------------------------------------------------
    let mut media_devices = env.create_object()?;

    let napi_class = NapiMediaStream::create_js_class(&env)?;
    media_devices.set_named_property("MediaStream", napi_class)?;

    media_devices.create_named_method("enumerateDevices", napi_enumerate_devices)?;
    media_devices.create_named_method("getUserMedia", napi_get_user_media)?;
    // expose media devices
    exports.set_named_property("mediaDevices", media_devices)?;

    // ----------------------------------------------------------------
    // Store constructors for classes that need to be created from within Rust code
    // ----------------------------------------------------------------
    let mut store = env.create_object()?;

    let napi_class = NapiAudioParam::create_js_class(&env)?;
    store.set_named_property("AudioParam", napi_class)?;

    let napi_class = NapiAudioDestinationNode::create_js_class(&env)?;
    store.set_named_property("AudioDestinationNode", napi_class)?;

    let napi_class = NapiAudioListener::create_js_class(&env)?;
    store.set_named_property("AudioListener", napi_class)?;

    let napi_class = NapiAudioBuffer::create_js_class(&env)?;
    store.set_named_property("AudioBuffer", napi_class)?;

    let napi_class = NapiMediaStream::create_js_class(&env)?;
    store.set_named_property("MediaStream", napi_class)?;

    // store the store into instance so that it can be globally accessed
    let store_ref = env.create_reference(store)?;
    env.set_instance_data(store_ref, 0, |mut c| {
        // don't have any idea of what this does
        c.value.unref(c.env).unwrap();
    })?;

    Ok(())
}

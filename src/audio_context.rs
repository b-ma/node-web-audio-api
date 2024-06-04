use std::io::Cursor;
use std::sync::Arc;

use napi::threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode};
use napi::*;
use napi_derive::js_function;
use web_audio_api::context::*;
use web_audio_api::Event;

use crate::*;

/// Napi object wrapping the native AudioContext and the AudioWorklet ID
#[derive(Clone)]
pub(crate) struct NapiAudioContext(Arc<AudioContext>, usize);

// for debug purpose
// impl Drop for NapiAudioContext {
//     fn drop(&mut self) {
//         println!("NAPI: NapiAudioContext dropped");
//     }
// }

impl NapiAudioContext {
    pub fn create_js_class(env: &Env) -> Result<JsFunction> {
        let interface = base_audio_context_interface![
            Property::new("baseLatency")?.with_getter(get_base_latency),
            Property::new("outputLatency")?.with_getter(get_output_latency),
            Property::new("sinkId")?.with_getter(get_sink_id),
            Property::new("setSinkId")?.with_method(set_sink_id),
            Property::new("resume")?.with_method(resume),
            Property::new("suspend")?.with_method(suspend),
            Property::new("close")?.with_method(close),
            // Workaround to bind the `sinkchange` and `statechange` events to EventTarget.
            // This must be called from JS facade ctor as the JS handler are added to the Napi
            // object after its instantiation, and that we don't have any initial `resume` call.
            Property::new("listen_to_events")?.with_method(listen_to_events)
        ];

        env.define_class("AudioContext", constructor, &interface)
    }

    pub fn unwrap(&self) -> &AudioContext {
        &self.0
    }

    pub fn worklet_id(&self) -> usize {
        self.1
    }
}

#[js_function(1)]
fn constructor(ctx: CallContext) -> Result<JsUndefined> {
    let mut js_this = ctx.this_unchecked::<JsObject>();

    // -------------------------------------------------
    // Parse options and create AudioContext
    // -------------------------------------------------
    let js_options = ctx.get::<JsObject>(0)?;

    let latency_hint_js = js_options
        .get::<&str, Either<JsString, JsNumber>>("latencyHint")?
        .unwrap();
    let latency_hint = match latency_hint_js {
        Either::A(js_string) => {
            let uf8_category = js_string.into_utf8()?.into_owned()?;
            let category = &uf8_category[..];

            match category {
                "interactive" => AudioContextLatencyCategory::Interactive,
                "balanced" => AudioContextLatencyCategory::Balanced,
                "playback" => AudioContextLatencyCategory::Playback,
                _ => unreachable!(),
            }
        }
        Either::B(js_number) => {
            let latency = js_number.get_double()?;
            AudioContextLatencyCategory::Custom(latency)
        }
    };

    let sample_rate_js = js_options.get::<&str, JsUnknown>("sampleRate")?.unwrap();
    let sample_rate = match sample_rate_js.get_type()? {
        ValueType::Number => {
            let sample_rate = sample_rate_js.coerce_to_number()?.get_double()? as f32;
            Some(sample_rate)
        }
        ValueType::Null => None,
        _ => unreachable!(),
    };

    let sink_id_js = js_options.get::<&str, JsString>("sinkId")?.unwrap();
    let sink_id_utf8 = sink_id_js.into_utf8()?.into_owned()?;
    let sink_id = sink_id_utf8.as_str().to_string();

    let audio_context_options = AudioContextOptions {
        latency_hint,
        sample_rate,
        sink_id,
        ..Default::default()
    };

    let audio_context = AudioContext::new(audio_context_options);
    let worklet_id = crate::audio_worklet_node::allocate_process_call_channel();

    // -------------------------------------------------
    // Wrap context
    // -------------------------------------------------
    let napi_audio_context = NapiAudioContext(Arc::new(audio_context), worklet_id);
    ctx.env.wrap(&mut js_this, napi_audio_context)?;

    js_this.define_properties(&[Property::new("Symbol.toStringTag")?
        .with_value(&ctx.env.create_string("AudioContext")?)
        .with_property_attributes(PropertyAttributes::Static)])?;

    // -------------------------------------------------
    // Bind AudioDestination and AudioRenderCapacity
    // -------------------------------------------------
    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;

    let ctor: JsFunction = store.get_named_property("AudioDestinationNode")?;
    let js_obj = ctor.new_instance(&[&js_this])?;
    js_this.set_named_property("destination", &js_obj)?;

    let ctor: JsFunction = store.get_named_property("AudioRenderCapacity")?;
    let js_obj = ctor.new_instance(&[&js_this])?;
    js_this.set_named_property("renderCapacity", &js_obj)?;

    // internal id to retrieve worklet message channel
    js_this.set_named_property("workletId", ctx.env.create_uint32(worklet_id as u32)?)?;

    ctx.env.get_undefined()
}

base_audio_context_impl!(NapiAudioContext);

#[js_function]
fn get_base_latency(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let obj = napi_obj.unwrap();

    let base_latency = obj.base_latency();
    ctx.env.create_double(base_latency)
}

#[js_function]
fn get_output_latency(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let obj = napi_obj.unwrap();

    let output_latency = obj.output_latency();
    ctx.env.create_double(output_latency)
}

#[js_function]
fn get_sink_id(ctx: CallContext) -> Result<JsString> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let obj = napi_obj.unwrap();

    let sink_id = obj.sink_id();
    ctx.env.create_string(&sink_id)
}

#[js_function(1)]
fn set_sink_id(ctx: CallContext) -> Result<JsUndefined> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let obj = napi_obj.unwrap();

    let sink_id_js = ctx.get::<JsString>(0)?;
    let sink_id = sink_id_js.into_utf8()?.into_owned()?;

    let res = obj.set_sink_id_sync(sink_id);

    if let Err(msg) = res {
        return Err(napi::Error::from_reason(msg.to_string()));
    }

    ctx.env.get_undefined()
}

#[js_function]
fn resume(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let context_clone = Arc::clone(&napi_context.0);

    ctx.env.execute_tokio_future(
        async move {
            context_clone.resume().await;
            Ok(())
        },
        |&mut env, _val| env.get_undefined(),
    )
}

#[js_function]
fn suspend(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let context_clone = Arc::clone(&napi_context.0);

    ctx.env.execute_tokio_future(
        async move {
            context_clone.suspend().await;
            Ok(())
        },
        |&mut env, _val| env.get_undefined(),
    )
}

#[js_function]
fn close(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let context_clone = Arc::clone(&napi_context.0);

    ctx.env.execute_tokio_future(
        async move {
            context_clone.close().await;
            Ok(())
        },
        |&mut env, _val| env.get_undefined(),
    )
}

// Workaround to bind the `sinkchange` and `statechange` events to EventTarget.
// This must be called from JS facade ctor as the JS handler are added to the Napi
// object after its instantiation, that we don't have any initial `resume` call,
// and in any case the statechange event should be called when audio device is ready.
#[js_function]
fn listen_to_events(ctx: CallContext) -> Result<JsUndefined> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiAudioContext>(&js_this)?;
    let context = napi_context.unwrap();

    let k_onsinkchange = crate::utils::get_symbol_for(ctx.env, "node-web-audio-api:onsinkchange");
    let sinkchange_cb = js_this.get_property(k_onsinkchange).unwrap();
    let mut sinkchange_tsfn = ctx.env.create_threadsafe_function(
        &sinkchange_cb,
        0,
        |ctx: ThreadSafeCallContext<Event>| {
            let mut event = ctx.env.create_object()?;
            let event_type = ctx.env.create_string(ctx.value.type_)?;
            event.set_named_property("type", event_type)?;
            Ok(vec![event])
        },
    )?;

    let k_onstatechange = crate::utils::get_symbol_for(ctx.env, "node-web-audio-api:onstatechange");
    let statechange_cb = js_this.get_property(k_onstatechange).unwrap();
    let mut statechange_tsfn = ctx.env.create_threadsafe_function(
        &statechange_cb,
        0,
        |ctx: ThreadSafeCallContext<Event>| {
            let mut event = ctx.env.create_object()?;
            let event_type = ctx.env.create_string(ctx.value.type_)?;
            event.set_named_property("type", event_type)?;

            Ok(vec![event])
        },
    )?;

    // unref tsfn so they do not prevent the process to exit
    let _ = sinkchange_tsfn.unref(ctx.env);
    let _ = statechange_tsfn.unref(ctx.env);

    context.set_onsinkchange(move |e| {
        sinkchange_tsfn.call(Ok(e), ThreadsafeFunctionCallMode::Blocking);
    });

    context.set_onstatechange(move |e| {
        statechange_tsfn.call(Ok(e), ThreadsafeFunctionCallMode::Blocking);
    });

    ctx.env.get_undefined()
}

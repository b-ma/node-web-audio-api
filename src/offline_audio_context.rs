use std::io::Cursor;
use std::sync::Arc;

use napi::threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode};
use napi::*;
use napi_derive::js_function;
use web_audio_api::context::*;
use web_audio_api::{Event, OfflineAudioCompletionEvent};

use crate::*;

/// Napi object wrapping the native OfflineAudioContext and the AudioWorklet ID
#[derive(Clone)]
pub(crate) struct NapiOfflineAudioContext(Arc<OfflineAudioContext>, usize);

// for debug purpose
// impl Drop for NapiOfflineAudioContext {
//     fn drop(&mut self) {
//         println!("NAPI: NapiOfflineAudioContext dropped");
//     }
// }

impl NapiOfflineAudioContext {
    pub fn create_js_class(env: &Env) -> Result<JsFunction> {
        let interface = base_audio_context_interface![
            Property::new("length")?.with_getter(get_length),
            Property::new("startRendering")?.with_method(start_rendering),
            Property::new("resume")?.with_method(resume),
            Property::new("suspend")?.with_method(suspend)
        ];

        env.define_class("OfflineAudioContext", constructor, &interface)
    }

    pub fn unwrap(&self) -> &OfflineAudioContext {
        &self.0
    }

    pub fn worklet_id(&self) -> usize {
        self.1
    }
}

#[js_function(3)]
fn constructor(ctx: CallContext) -> Result<JsUndefined> {
    let mut js_this = ctx.this_unchecked::<JsObject>();

    // -------------------------------------------------
    // Parse options and create OfflineAudioContext
    // -------------------------------------------------
    let number_of_channels = ctx.get::<JsNumber>(0)?.get_double()? as usize;
    let length = ctx.get::<JsNumber>(1)?.get_double()? as usize;
    let sample_rate = ctx.get::<JsNumber>(2)?.get_double()? as f32;

    let audio_context = OfflineAudioContext::new(number_of_channels, length, sample_rate);
    let worklet_id = crate::audio_worklet_node::allocate_process_call_channel();

    // -------------------------------------------------
    // Wrap context
    // -------------------------------------------------
    let napi_audio_context = NapiOfflineAudioContext(Arc::new(audio_context), worklet_id);
    ctx.env.wrap(&mut js_this, napi_audio_context)?;

    js_this.define_properties(&[
        // this must be put on the instance and not in the prototype to be reachable
        Property::new("Symbol.toStringTag")?
            .with_value(&ctx.env.create_string("OfflineAudioContext")?)
            .with_property_attributes(PropertyAttributes::Static),
    ])?;

    // -------------------------------------------------
    // Bind AudioDestination - requires Symbol.toStringTag
    // -------------------------------------------------
    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("AudioDestinationNode")?;
    let js_obj = ctor.new_instance(&[&js_this])?;
    js_this.set_named_property("destination", &js_obj)?;

    // internal id to retrieve worklet message channel
    js_this.set_named_property("workletId", ctx.env.create_uint32(worklet_id as u32)?)?;

    ctx.env.get_undefined()
}

base_audio_context_impl!(NapiOfflineAudioContext);

#[js_function]
fn get_length(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<NapiOfflineAudioContext>(&js_this)?;
    let obj = napi_obj.unwrap();

    let length = obj.length() as f64;
    ctx.env.create_double(length)
}

#[js_function]
fn start_rendering(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiOfflineAudioContext>(&js_this)?;
    let context = napi_context.unwrap();

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

    let k_oncomplete = crate::utils::get_symbol_for(ctx.env, "node-web-audio-api:oncomplete");
    let complete_cb = js_this.get_property(k_oncomplete).unwrap();
    let mut complete_tsfn = ctx.env.create_threadsafe_function(
        &complete_cb,
        0,
        |ctx: ThreadSafeCallContext<OfflineAudioCompletionEvent>| {
            let raw_event = ctx.value;
            let mut event = ctx.env.create_object()?;

            let event_type = ctx.env.create_string("complete")?;
            event.set_named_property("type", event_type)?;

            // @fixme: this event is propagated before `startRedering` fulfills
            // which is probaly wrong, so let's propagate the JS audio buffer
            // and let JS handle the race condition
            let ctor = crate::utils::get_class_ctor(&ctx.env, "AudioBuffer")?;
            let js_audio_buffer = ctor.new_instance(&[ctx.env.get_null()?])?;
            let napi_audio_buffer = ctx.env.unwrap::<NapiAudioBuffer>(&js_audio_buffer)?;
            napi_audio_buffer.insert(raw_event.rendered_buffer);

            event.set_named_property("renderedBuffer", js_audio_buffer)?;

            Ok(vec![event])
        },
    )?;

    // unref tsfn so they do not prevent the process to exit
    let _ = statechange_tsfn.unref(ctx.env);
    let _ = complete_tsfn.unref(ctx.env);

    context.set_onstatechange(move |e| {
        statechange_tsfn.call(Ok(e), ThreadsafeFunctionCallMode::Blocking);
    });

    context.set_oncomplete(move |e| {
        complete_tsfn.call(Ok(e), ThreadsafeFunctionCallMode::Blocking);
    });

    // everything is setup, do "real" rendering job
    let context_clone = Arc::clone(&napi_context.0);

    ctx.env.execute_tokio_future(
        async move {
            let audio_buffer = context_clone.start_rendering().await;
            Ok(audio_buffer)
        },
        |&mut env, audio_buffer| {
            // create Napi audio buffer from native audio buffer
            let ctor = crate::utils::get_class_ctor(&env, "AudioBuffer")?;
            let js_audio_buffer = ctor.new_instance(&[env.get_null()?])?;
            let napi_audio_buffer = env.unwrap::<NapiAudioBuffer>(&js_audio_buffer)?;
            napi_audio_buffer.insert(audio_buffer);

            Ok(js_audio_buffer)
        },
    )
}

#[js_function]
fn resume(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiOfflineAudioContext>(&js_this)?;
    let context_clone = Arc::clone(&napi_context.0);

    ctx.env.execute_tokio_future(
        async move {
            context_clone.resume().await;
            Ok(())
        },
        |&mut env, _val| env.get_undefined(),
    )
}

#[js_function(1)]
fn suspend(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_context = ctx.env.unwrap::<NapiOfflineAudioContext>(&js_this)?;
    let context_clone = Arc::clone(&napi_context.0);

    let when = ctx.get::<JsNumber>(0)?.get_double()?;

    ctx.env.execute_tokio_future(
        async move {
            context_clone.suspend(when).await;
            Ok(())
        },
        |&mut env, _val| env.get_undefined(),
    )
}

use std::io::Cursor;

use napi::*;
use napi_derive::js_function;
use web_audio_api::context::*;

use crate::*;

pub(crate) struct ${d.napiName(d.node)}(${d.name(d.node)});

impl ${d.napiName(d.node)} {
    pub fn create_js_class(env: &Env) -> Result<JsFunction> {
        env.define_class(
            "${d.name(d.node)}",
            constructor,
            &[
                Property::new("currentTime")?.with_getter(get_current_time),
                Property::new("sampleRate")?.with_getter(get_sample_rate),
                Property::new("listener")?.with_getter(get_listener),

                Property::new("decodeAudioData")?.with_method(decode_audio_data),
                Property::new("createPeriodicWave")?.with_method(create_periodic_wave),
                Property::new("createBuffer")?.with_method(create_buffer),

                // ----------------------------------------------------
                // Factory methods
                // ----------------------------------------------------
                ${d.nodes.map(n => {
                    let factory = d.factoryName(n);
                    return `
                Property::new("${factory}")?.with_method(${d.slug(factory)}),`
                }).join('')}


                ${d.name(d.node) === 'AudioContext' ?
                    `
                // @todo - expose in OfflineAudioContext as well
                Property::new("state")?.with_getter(get_state),
                Property::new("resume")?.with_method(resume),
                Property::new("suspend")?.with_method(suspend),
                Property::new("close")?.with_method(close),
                // event
                // Property::new("__onstatechange")?.with_setter(onstatechange),

                // ----------------------------------------------------
                // Methods and attributes specific to AudioContext
                // ----------------------------------------------------
                Property::new("baseLatency")?.with_getter(get_base_latency),
                Property::new("outputLatency")?.with_getter(get_output_latency),
                Property::new("setSinkId")?.with_method(set_sink_id),
                Property::new("createMediaStreamSource")?.with_method(create_media_stream_source),
                    ` : `
                // ----------------------------------------------------
                // Methods and attributes specifc to OfflineAudioContext
                // ----------------------------------------------------
                Property::new("length")?.with_getter(get_length),
                Property::new("startRendering")?.with_method(start_rendering),
                    `
                }
            ],
        )
    }

    pub fn unwrap(&self) -> &${d.name(d.node)} {
        &self.0
    }
}

${d.name(d.node) === 'AudioContext' ? `#[js_function(1)]` : `#[js_function(3)]`}
fn constructor(ctx: CallContext) -> Result<JsUndefined> {
    let mut js_this = ctx.this_unchecked::<JsObject>();

    ${d.name(d.node) === 'AudioContext' ?
        `
    // -------------------------------------------------
    // Parse options and create AudioContext
    // -------------------------------------------------
    let options_js: Option<JsObject> = ctx.try_get::<JsObject>(0)?.into();
    let audio_context_options = if let Some(options) = options_js {
        // LatencyHint
        let latency_hint = if let Some(latency_hint_js) =
            options.get::<&str, Either<JsString, JsNumber>>("latencyHint")?
        {
            match latency_hint_js {
                Either::A(js_string) => {
                    let uf8_category = js_string.into_utf8()?.into_owned()?;
                    let category = &uf8_category[..];

                    match category {
                        "interactive" => AudioContextLatencyCategory::Interactive,
                        "balanced" => AudioContextLatencyCategory::Balanced,
                        "playback" => AudioContextLatencyCategory::Playback,
                        _ => AudioContextLatencyCategory::Interactive, // default
                    }
                }
                Either::B(js_number) => {
                    let latency = js_number.get_double()?;
                    AudioContextLatencyCategory::Custom(latency)
                }
            }
        } else {
            AudioContextLatencyCategory::Interactive
        };

        let sample_rate =
            if let Some(sample_rate_js) = options.get::<&str, JsNumber>("sampleRate")? {
                let sample_rate = sample_rate_js.get_double()? as f32;
                Some(sample_rate)
            } else {
                None
            };

        let sink_id_js = options.get::<&str, JsString>("sinkId")?;
        let sink_id = if let Some(sink_id_js) = sink_id_js {
            let sink_id_utf8 = sink_id_js.into_utf8()?.into_owned()?;
            sink_id_utf8.as_str().to_string()
        } else {
            String::new()
        };

        AudioContextOptions {
            latency_hint,
            sample_rate,
            sink_id,
            ..Default::default()
        }
    } else {
        AudioContextOptions::default()
    };

    let audio_context = ${d.name(d.node)}::new(audio_context_options);
        ` : `
    // -------------------------------------------------
    // Parse options and create OfflineAudioContext
    // -------------------------------------------------
    let number_of_channels = ctx.get::<JsNumber>(0)?.get_double()? as usize;
    let length = ctx.get::<JsNumber>(1)?.get_double()? as usize;
    let sample_rate = ctx.get::<JsNumber>(2)?.get_double()? as f32;

    let audio_context = ${d.name(d.node)}::new(number_of_channels, length, sample_rate);
        `}

    // -------------------------------------------------
    // Wrap context
    // -------------------------------------------------
    let napi_audio_context = ${d.napiName(d.node)}(audio_context);
    ctx.env.wrap(&mut js_this, napi_audio_context)?;

    js_this.define_properties(&[
        // this must be put on the instance and not in the prototype to be reachable
        Property::new("Symbol.toStringTag")?
            .with_value(&ctx.env.create_string("${d.name(d.node)}")?)
            .with_property_attributes(PropertyAttributes::Static),
    ])?;


    // -------------------------------------------------
    // Bind AudioDestination
    // -------------------------------------------------
    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("AudioDestinationNode")?;
    let js_obj = ctor.new_instance(&[&js_this])?;
    js_this.set_named_property("destination", &js_obj)?;

    ctx.env.get_undefined()
}

#[js_function]
fn get_current_time(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let current_time = obj.current_time();
    ctx.env.create_double(current_time)
}

#[js_function]
fn get_sample_rate(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let sample_rate = obj.sample_rate() as f64;
    ctx.env.create_double(sample_rate)
}

// use a getter so we can lazily create the listener on first call and retrieve it afterward
#[js_function]
fn get_listener(ctx: CallContext) -> Result<JsObject> {
    let mut js_this = ctx.this_unchecked::<JsObject>();

    // reproduce lazy instanciation strategy from rust crate
    if js_this.has_named_property("__listener__").ok().unwrap() {
        js_this.get_named_property("__listener__")
    } else {
        let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
        let store: JsObject = ctx.env.get_reference_value(store_ref)?;
        let ctor: JsFunction = store.get_named_property("AudioListener")?;
        let js_obj = ctor.new_instance(&[&js_this])?;
        js_this.set_named_property("__listener__", &js_obj)?;

        Ok(js_obj)
    }
}

// ----------------------------------------------------
// METHODS
// ----------------------------------------------------

// @todo - async version
#[js_function(1)]
fn decode_audio_data(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let context = napi_obj.unwrap();

    let js_buffer = ctx.get::<JsArrayBuffer>(0)?.into_value()?;
    let cursor = Cursor::new(js_buffer.to_vec());
    let audio_buffer = context.decode_audio_data_sync(cursor);

    match audio_buffer {
        Ok(audio_buffer) => {
            // create js audio buffer instance
            let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
            let store: JsObject = ctx.env.get_reference_value(store_ref)?;
            let ctor: JsFunction = store.get_named_property("AudioBuffer")?;
            let mut options = ctx.env.create_object()?;
            options.set("__internal_caller__", ctx.env.get_null())?;

            // populate with audio buffer
            let js_audio_buffer = ctor.new_instance(&[options])?;
            let napi_audio_buffer = ctx.env.unwrap::<NapiAudioBuffer>(&js_audio_buffer)?;
            napi_audio_buffer.populate(audio_buffer);

            Ok(js_audio_buffer)
        },
        Err(e) => {
            Err(napi::Error::from_reason(e.to_string()))
        },
    }
}

#[js_function(3)]
fn create_buffer(ctx: CallContext) -> Result<JsObject> {
    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("AudioBuffer")?;

    let number_of_channels = ctx.get::<JsNumber>(0)?;
    let length = ctx.get::<JsNumber>(1)?;
    let sample_rate = ctx.get::<JsNumber>(2)?;

    let mut options = ctx.env.create_object()?;
    options.set("numberOfChannels", number_of_channels)?;
    options.set("length", length)?;
    options.set("sampleRate", sample_rate)?;

    ctor.new_instance(&[options])
}

#[js_function(3)]
fn create_periodic_wave(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();

    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("PeriodicWave")?;

    let real = ctx.get::<JsTypedArray>(0)?;
    let imag = ctx.get::<JsTypedArray>(1)?;
    // this differ slightly from the spec
    let disable_normalization = match ctx.try_get::<JsObject>(2)? {
        Either::A(constraints_js) => {
            if let Some(disable_nomalization) = constraints_js.get::<&str, JsBoolean>("disableNormalization")? {
                disable_nomalization
            } else {
                ctx.env.get_boolean(false)?
            }
        },
        Either::B(_) => ctx.env.get_boolean(false)?,
    };

    let mut options = ctx.env.create_object()?;
    options.set("real", real)?;
    options.set("imag", imag)?;
    options.set("disableNormalization", disable_normalization)?;

    ctor.new_instance(&[js_this, options])
}

// ----------------------------------------------------
// Factory methods
// ----------------------------------------------------
${d.nodes.map(n => {
    let factoryName = d.factoryName(n);
    let factoryIdl = d.factoryIdl(factoryName);
    let args = factoryIdl.arguments;

    return `
#[js_function(${args.length})]
fn ${d.slug(factoryName)}(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();

    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("${d.name(n)}")?;

    ${args.length > 0 ?
        `let mut options = ctx.env.create_object()?;
        ${args.map((arg, index) => {
            switch (arg.idlType.idlType) {
                case 'unsigned long': // channel merger, channel splitter
                case 'double': // delay
                    return `
    match ctx.try_get::<JsNumber>(${index})? {
        Either::A(value) => options.set("${arg.name}", value)?,
        Either::B(_) => ()
    }
                `
                    break;
                default:
                    // IIR Filter
                    if (arg.idlType.generic == 'sequence' &&  arg.idlType.idlType[0].idlType === 'double') {
                        return `
                            match ctx.try_get::<JsTypedArray>(${index})? {
                                Either::A(value) => options.set("${arg.name}", value)?,
                                Either::B(_) => ()
                            }
                        `
                    } else {
                        console.log(`[factory] argument ${idl.name} for ${factoryName} not parsed`);
                    }

                    break;
        }}).join('')}
    ctor.new_instance(&[js_this, options])` : `ctor.new_instance(&[js_this])`
    }
}
    `;
}).join('')}


${d.name(d.node) === 'AudioContext' ?
    `
// ----------------------------------------------------
// Methods and attributes specific to AudioContext
// ----------------------------------------------------

// @todo - expose in OfflineAudioContext
// see https://webaudio.github.io/web-audio-api/#dom-baseaudiocontext-state
#[js_function]
fn get_state(ctx: CallContext) -> Result<JsString> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let state = obj.state();
    let state_str = match state {
        AudioContextState::Suspended => "suspended",
        AudioContextState::Running => "running",
        AudioContextState::Closed => "closed",
    };

    ctx.env.create_string(state_str)
}

${['resume', 'suspend', 'close'].map(method => `
// @todo - async version
#[js_function]
fn ${method}(ctx: CallContext) -> Result<JsUndefined> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    obj.${method}_sync();

    ctx.env.get_undefined()
}
`).join('')}

// #[js_function]
// fn onstatechange(ctx: CallContext) -> Result<JsUndefined> {
//     let js_this = ctx.this_unchecked::<JsObject>();
//     let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
//     let obj = napi_obj.unwrap();



//     ctx.env.get_undefined();
// }

#[js_function]
fn get_base_latency(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let base_latency = obj.base_latency();
    ctx.env.create_double(base_latency)
}

#[js_function]
fn get_output_latency(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let output_latency = obj.output_latency();
    ctx.env.create_double(output_latency)
}

#[js_function(1)]
fn set_sink_id(ctx: CallContext) -> Result<JsUndefined> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let sink_id_js = ctx.get::<JsString>(0)?;
    let sink_id = sink_id_js.into_utf8()?.into_owned()?;

    let res = obj.set_sink_id_sync(sink_id);

    if let Err(msg) = res {
        return Err(napi::Error::from_reason(msg.to_string()));
    }

    ctx.env.get_undefined()
}

#[js_function(1)]
fn create_media_stream_source(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();

    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("MediaStreamAudioSourceNode")?;

    let media_stream = ctx.get::<JsObject>(0)?;

    // create options object according to MediaStreamAudioSourceNode ctor API
    let mut options = ctx.env.create_object()?;
    options.set("mediaStream", media_stream)?;

    ctor.new_instance(&[js_this, options])
}
    `: `
// ----------------------------------------------------
// Methods and attributes specific to OfflineAudioContext
// ----------------------------------------------------

#[js_function]
fn get_length(ctx: CallContext) -> Result<JsNumber> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;
    let obj = napi_obj.unwrap();

    let length = obj.length() as f64;
    ctx.env.create_double(length)
}

#[js_function]
fn start_rendering(ctx: CallContext) -> Result<JsObject> {
    let js_this = ctx.this_unchecked::<JsObject>();
    let napi_obj = ctx.env.unwrap::<${d.napiName(d.node)}>(&js_this)?;

    let audio_buffer = napi_obj.0.start_rendering_sync();

    // create js audio buffer instance
    let store_ref: &mut napi::Ref<()> = ctx.env.get_instance_data()?.unwrap();
    let store: JsObject = ctx.env.get_reference_value(store_ref)?;
    let ctor: JsFunction = store.get_named_property("AudioBuffer")?;
    let mut options = ctx.env.create_object()?;
    options.set("__internal_caller__", ctx.env.get_null())?;

    // populate with audio buffer
    let js_audio_buffer = ctor.new_instance(&[options])?;
    let napi_audio_buffer = ctx.env.unwrap::<NapiAudioBuffer>(&js_audio_buffer)?;
    napi_audio_buffer.populate(audio_buffer);

    Ok(js_audio_buffer)
}
    `
}

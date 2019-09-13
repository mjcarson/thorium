//! Sets up tracing for Thorium using either jaeger or stdout/stderr

use opentelemetry::sdk::trace::BatchConfig;
use opentelemetry::sdk::Resource;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use std::path::Path;
use tracing::Span;
use tracing_core::LevelFilter;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{filter::Filtered, fmt::Layer, layer::Layered, prelude::*, Registry};

use crate::conf::{LogLevel, Tracing, TracingLocal, TracingServices};

/// Log a message at the info level
#[macro_export]
macro_rules! info {
    ($level:expr, $($msg:tt)+) => {
        if $level ==  crate::conf::LogLevel::Info
        || $level ==  crate::conf::LogLevel::Debug
        || $level ==  crate::conf::LogLevel::Trace  {
            println!("{}", serde_json::json!({"timestamp": chrono::Utc::now(), "level": "INFO", "msg": $($msg)+}));
        }
    }
}

/// Log a message at the info level
#[macro_export]
macro_rules! setup {
    ($level:expr, $($msg:tt)+) => {
        if $level ==  crate::conf::LogLevel::Setup
        || $level ==  crate::conf::LogLevel::Info
        || $level ==  crate::conf::LogLevel::Debug
        || $level ==  crate::conf::LogLevel::Trace  {
            println!("{}", serde_json::json!({"timestamp": chrono::Utc::now(), "level": "SETP", "msg": $($msg)+}));
        }
    }
}

/// Log a message at the error level
#[macro_export]
macro_rules! error {
    ($level:expr, $($msg:tt)+) => {
        if $level !=  crate::conf::LogLevel::Off {
            println!("{}", serde_json::json!({"timestamp": chrono::Utc::now(), "level": "ERRO", "msg": $($msg)+}));
        }
    }
}

/// Get the current traces id
pub fn get_trace() -> Option<String> {
    // get our current context and span
    let context = Span::current().context();
    let span = context.span();
    // get this spans context
    let span_context = span.span_context();
    // try to extract our trace id
    span_context
        .is_valid()
        .then(|| span_context.trace_id().to_string())
}

/// Setup our jaeger tracer
///
/// This talks directly to a jaeger collector
///
/// # Arguments
///
/// * `name` - The name of the service to trace
/// * `collector` - The collector endpoint to send traces too
/// * `level` - The log level to set
/// * `registry` - The registry to add our tracers too
fn setup_jaeger(
    name: &str,
    collector: &str,
    level: LogLevel,
    registry: Layered<Filtered<Layer<Registry>, LevelFilter, Registry>, Registry>,
) {
    // build the endpoint to send traces too
    let endpoint = format!("http://{}/api/traces", collector);
    // setup our tracer
    let tracer = opentelemetry_jaeger::new_collector_pipeline()
        .with_endpoint(endpoint)
        .with_service_name(name)
        .with_hyper()
        .with_batch_processor_config(
            BatchConfig::default()
                .with_max_queue_size(8192)
                .with_max_concurrent_exports(10),
        )
        .install_batch(opentelemetry::runtime::Tokio)
        .expect("Failed to setup tracer");
    // build our tracing layer
    let filtered = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(level.to_filter());
    // init our tracing registry
    registry
        .with(filtered)
        .try_init()
        .expect("Failed to register opentelemetry tracers/subscribers");
    info!(
        level,
        format!(
            "Sending {} traces for {} to jaeger at {}",
            level, name, collector
        )
    );
}

/// Setup our grpc tracer.
///
/// # Arguments
///
/// * `name` - The name of the service to trace
/// * `endpoint` - The gRPC endpoint to send traces too
/// * `level` - The log level to set
/// * `registry` - The registry to add our tracers too
fn setup_grpc(
    name: &str,
    endpoint: &str,
    level: LogLevel,
    registry: Layered<Filtered<Layer<Registry>, LevelFilter, Registry>, Registry>,
) {
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint);
    // setup our tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            opentelemetry::sdk::trace::config().with_resource(Resource::new(vec![KeyValue::new(
                SERVICE_NAME,
                name.to_string(),
            )])),
        )
        .install_batch(opentelemetry::runtime::Tokio)
        .expect("Failed to setup tracer");
    // define our layer filter
    let filtered = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(level.to_filter());
    // init our tracing registry
    registry
        .with(filtered)
        .try_init()
        .expect("Failed to register opentelemetry tracers/subscribers");
    info!(
        level,
        format!(
            "Sending {} traces for {} to gRPC trace sink at {}",
            level, name, endpoint
        )
    );
}

/// Setup our local tracer
///
/// # Arguments
///
/// * `shared` - Shared Thorium objects
fn setup_local(
    name: &str,
    conf: &TracingLocal,
) -> Filtered<Layer<Registry>, LevelFilter, Registry> {
    // log that local tracing is enabled
    info!(
        conf.level,
        format!("Sending {} for {name} to stdout", conf.level)
    );
    tracing_subscriber::fmt::layer().with_filter(conf.level.to_filter())
}

/// Setup the correct tracer
///
/// # Arguments
///
/// * `conf` - The Thorium config
pub fn setup(name: &str, trace_conf: &Tracing) {
    // build our local tracer/subscriber
    let local = setup_local(name, &trace_conf.local);
    // Add our local tracer to our registry
    let registry = tracing_subscriber::registry().with(local);
    // get out external tracing settings
    if let Some(external) = &trace_conf.external {
        match external {
            // setup the correct external tracer
            TracingServices::Jaeger { collector, level } => {
                setup_jaeger(name, collector, *level, registry)
            }
            TracingServices::Grpc { endpoint, level } => {
                setup_grpc(name, endpoint, *level, registry)
            }
        }
    } else {
        registry
            .try_init()
            .expect("Failed to register stdout registry");
    };
}

/// Setup the correct tracer from a stand alone config file
///
/// # Arguments
///
/// * `conf` - The Thorium config
pub fn from_file(name: &str, path: &str) {
    // Check if our tracing file exists or not
    let trace_config = if Path::new(path).exists() {
        // load our tracing config from a file
        Tracing::from_file(path).expect("Failed to load tracing config")
    } else {
        // use a default config
        Tracing::default()
    };
    // setup our tracers/subscribers
    setup(name, &trace_config);
}

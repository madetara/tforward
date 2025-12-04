use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{
    LogExporterBuilder, SpanExporterBuilder, WithExportConfig, WithTonicConfig,
};
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    resource::{
        EnvResourceDetector, ResourceDetector, SdkProvidedResourceDetector,
        TelemetryResourceDetector,
    },
    trace::{RandomIdGenerator, SdkTracerProvider},
};
use tonic::{metadata::MetadataMap, transport::ClientTlsConfig};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Registry, fmt, prelude::*};

pub fn setup() {
    let dsn = std::env::var("UPTRACE_DSN").expect("UPTRACE_DSN not set");
    let mut metadata = MetadataMap::with_capacity(1);
    metadata.insert("uptrace-dsn", dsn.parse().unwrap());

    let detectors: Vec<Box<dyn ResourceDetector>> = vec![
        Box::new(SdkProvidedResourceDetector),
        Box::new(EnvResourceDetector::new()),
        Box::new(TelemetryResourceDetector),
    ];
    let resource = Resource::builder().with_detectors(&detectors).build();
    let tracer_provider = init_tracer(&resource, &metadata);
    let logger_provider = init_logger(&resource, &metadata);

    let tracer = tracer_provider.tracer("tforward");
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let subscriber = Registry::default()
        .with(telemetry.with_filter(LevelFilter::INFO))
        .with(OpenTelemetryTracingBridge::new(&logger_provider).with_filter(LevelFilter::INFO))
        .with(fmt::Layer::default().with_filter(LevelFilter::DEBUG));

    tracing::subscriber::set_global_default(subscriber).unwrap();
}

fn init_tracer(resource: &Resource, metadata: &MetadataMap) -> SdkTracerProvider {
    let exporter = SpanExporterBuilder::new()
        .with_tonic()
        .with_tls_config(ClientTlsConfig::new().with_native_roots())
        .with_endpoint("https://otlp.uptrace.dev:4317")
        .with_timeout(Duration::from_secs(5))
        .with_metadata(metadata.clone())
        .build()
        .unwrap();

    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource.clone())
        .with_id_generator(RandomIdGenerator::default())
        .build()
}

fn init_logger(resource: &Resource, metadata: &MetadataMap) -> SdkLoggerProvider {
    let exporter = LogExporterBuilder::new()
        .with_tonic()
        .with_tls_config(ClientTlsConfig::new().with_native_roots())
        .with_endpoint("https://otlp.uptrace.dev:4317")
        .with_timeout(Duration::from_secs(5))
        .with_metadata(metadata.clone())
        .build()
        .unwrap();

    SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource.clone())
        .build()
}

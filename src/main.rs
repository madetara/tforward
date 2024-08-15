mod bot;
mod telemetry;

#[tokio::main]
async fn main() {
    openssl_probe::init_ssl_cert_env_vars();

    telemetry::setup();

    tracing::info!("starting");

    bot::run().await.unwrap();
}

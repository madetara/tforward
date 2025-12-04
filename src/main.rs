mod bot;
mod telemetry;

#[tokio::main]
async fn main() {
    unsafe {
        openssl_probe::init_openssl_env_vars();
    }

    telemetry::setup();

    tracing::info!("starting");

    bot::run().await.unwrap();
}

// import any API only structures
cfg_if::cfg_if! {
    if #[cfg(feature = "api")] {
        mod args;

        use clap::Parser;
    }
}

#[cfg(feature = "api")]
/// Start Thorium Api
#[tokio::main]
async fn main() {
    // load command line args
    let args = args::Args::parse();
    // load config
    let conf = thorium::conf::Conf::new(&args.config).expect("Failed to load config");
    // launch our api
    Box::pin(thorium::axum(conf)).await;
}

/// Main function alerting the user to compile the API with the api feature enabled
#[cfg(not(feature = "api"))]
fn main() {
    println!("To run the Thorium API please enable the API feature");
}

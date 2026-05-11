//! The Thorium event handler

use clap::Parser;

mod args;
mod libs;

use libs::{EventWorkerCache, EventWorkerController};
use thorium::{conf::Thorium, models::SigmaRuleAppliesTo};

#[tokio::main]
async fn main() {
    // get command line args
    let args = args::Args::parse();
    //// get a Thorium client
    //let thorium = thorium::Thorium::from_ctl_conf_file("/Users/mcarson/.thorium/config.yml")
    //    .await
    //    .unwrap();
    //// build a sigma rule context
    //let sigma_ctx = libs::workers::hunts::sigma::SigmaRuleContext::new(&thorium)
    //    .await
    //    .unwrap();
    //// build a sigma worker
    //let worker = libs::workers::hunts::sigma::SigmaRuleWorker::new(sigma_ctx, &thorium);
    //// build a pretend event
    //let event = libs::workers::hunts::sigma::SigmaRuleEvent {
    //    result_key: libs::workers::hunts::sigma::ResultKey::Sample(
    //        "7e724ff06c0967416958752aee8569c4bbc3ea733d572a9d870ebbfedcdf553d".to_owned(),
    //    ),
    //    applies_to: vec![SigmaRuleAppliesTo::WindowsProcesses],
    //    tool: "auto-volatility3".to_owned(),
    //};
    //// scan this pretend event
    //worker.scan_event(event).await.unwrap();
    // try to load a config file
    let conf = thorium::Conf::new(&args.config).expect("Failed to load config");
    // setup our tracer
    let trace_provider = thorium::utils::trace::setup("ThoriumEventHandler", &conf.thorium.tracing);
    // build our event controller
    let controller = EventWorkerController::new(args, conf)
        .await
        .expect("Failed to start event controller");
    // start our event handler workers
    controller.start().await.expect("Event controller failed!");
    // export any remaining traces and shutdown this provider
    thorium::utils::trace::shutdown(trace_provider);
}

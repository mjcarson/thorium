#![feature(round_char_boundary)]

use clap::Parser;
use thorium::Conf;

mod args;
mod monitor;
mod msg;
mod sources;
mod stores;
mod streamer;
mod worker;

use args::Args;
use streamer::{Elastic, SamplesOutput, SearchStreamer};

#[tokio::main]
async fn main() {
    // get our command line args
    let args = Args::parse();
    // load our config
    let conf = Conf::new(&args.config).expect("Failed to load config");
    // setup our tracer
    thorium::utils::trace::setup("ThoriumSearchStreamer", &conf.thorium.tracing);
    // build an elastic streamer
    let streamer = SearchStreamer::<SamplesOutput, Elastic>::new(&args, conf)
        .await
        .unwrap();
    // start our streamer
    streamer.start().await.unwrap();
}

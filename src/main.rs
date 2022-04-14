use serde_json::json;
use structopt::StructOpt;
use tiny_http::{Response, Server};

const WEBHOOK: &'static str = "127.0.0.1:3000";

#[derive(Debug, StructOpt)]
#[structopt(name = "Uzi Miner", about = "Mine Zeeka with Uzi!")]
struct Opt {
    #[structopt(short = "t", long = "threads", default_value = "1")]
    threads: usize,

    #[structopt(short = "n", long = "node")]
    node: String,
}

fn main() {
    let opt = Opt::from_args();

    // Register the miner webhook on the node software
    ureq::post(&format!("{}/miner", opt.node))
        .send_json(json!({ "webhook": format!("https://{}", WEBHOOK) }))
        .unwrap();

    let server = Server::http(WEBHOOK).unwrap();

    for request in server.incoming_requests() {
        let response = Response::from_string("hello world");

        request.respond(response).unwrap();
    }
}

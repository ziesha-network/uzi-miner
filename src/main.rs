use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::thread;
use structopt::StructOpt;
use tiny_http::{Response, Server};

mod miner;

const WEBHOOK: &'static str = "127.0.0.1:3000";

#[derive(Debug, StructOpt)]
#[structopt(name = "Uzi Miner", about = "Mine Zeeka with Uzi!")]
struct Opt {
    #[structopt(short = "t", long = "threads", default_value = "1")]
    threads: usize,

    #[structopt(short = "n", long = "node")]
    node: String,

    #[structopt(long = "slow")]
    slow: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct Request {
    key: String,
    blob: String,
    offset: usize,
    size: usize,
    target: u32,
}

fn main() {
    let opt = Opt::from_args();

    // Register the miner webhook on the node software
    ureq::post(&format!("{}/miner", opt.node))
        .send_json(json!({ "webhook": format!("https://{}", WEBHOOK) }))
        .unwrap();

    let workers = (0..opt.threads)
        .map(|_| miner::Worker::new())
        .collect::<Vec<_>>();

    let server = Server::http(WEBHOOK).unwrap();

    let mut context: Option<Arc<rust_randomx::Context>> = None;

    let (sol_send, sol_recv) = std::sync::mpsc::channel::<miner::Solution>();

    let solution_getter = thread::spawn(|| {
        for sol in sol_recv {
            println!("Solution found!: {:?}", sol);
        }
    });

    for mut request in server.incoming_requests() {
        let mut content = String::new();
        request.as_reader().read_to_string(&mut content).unwrap();
        let req: Request = serde_json::from_str(&content).unwrap();

        let req_key = hex::decode(&req.key).unwrap();
        if context.is_none() || context.as_ref().unwrap().key() == req_key {
            context = Some(Arc::new(rust_randomx::Context::new(&req_key, !opt.slow)));
        }

        for w in workers.iter() {
            w.chan
                .send(miner::Puzzle {
                    context: Arc::clone(context.as_ref().unwrap()),
                    blob: hex::decode(&req.blob).unwrap(),
                    offset: req.offset,
                    count: req.size,
                    target: rust_randomx::Difficulty::new(req.target),
                    callback: sol_send.clone(),
                })
                .unwrap();
        }

        request.respond(Response::from_string("OK")).unwrap();
    }

    for w in workers {
        w.handle.join().unwrap();
    }
    drop(sol_send);
    solution_getter.join().unwrap();
}

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::thread;
use structopt::StructOpt;
use tiny_http::{Response, Server};

mod miner;

const WEBHOOK: &'static str = "127.0.0.1:3000";

#[derive(Debug, StructOpt, Clone)]
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

    let server = Server::http(WEBHOOK).unwrap();

    let workers = Arc::new(Mutex::new(Vec::<miner::Worker>::new()));
    let mut puzzle_id = 0;

    let mut context: Option<Arc<rust_randomx::Context>> = None;

    let (sol_send, sol_recv) = std::sync::mpsc::channel::<miner::Solution>();

    let solution_getter = {
        let workers = Arc::clone(&workers);
        let opt = opt.clone();
        thread::spawn(move || {
            for sol in sol_recv {
                println!("Found solution!");
                workers
                    .lock()
                    .unwrap()
                    .retain(|w| w.send(miner::Message::Break).is_ok());
                ureq::post(&format!("{}/miner/mine", opt.node))
                    .send_json(json!({ "nonce": sol.nonce }))
                    .unwrap();
            }
        })
    };

    for mut request in server.incoming_requests() {
        // Parse request
        let req: Request = {
            let mut content = String::new();
            request.as_reader().read_to_string(&mut content).unwrap();
            serde_json::from_str(&content).unwrap()
        };

        // Reinitialize context if needed
        let req_key = hex::decode(&req.key).unwrap();
        if context.is_none() || context.as_ref().unwrap().key() != req_key {
            context = Some(Arc::new(rust_randomx::Context::new(&req_key, !opt.slow)));
        }

        // Ensure correct number of workers
        let mut workers = workers.lock().unwrap();
        while workers.len() < opt.threads {
            workers.push(miner::Worker::new(sol_send.clone()));
        }

        // Send the puzzle to workers
        workers.retain(|w| {
            w.send(miner::Message::Puzzle(miner::Puzzle {
                id: puzzle_id,
                context: Arc::clone(context.as_ref().unwrap()),
                blob: hex::decode(&req.blob).unwrap(),
                offset: req.offset,
                count: req.size,
                target: rust_randomx::Difficulty::new(req.target),
            }))
            .is_ok()
        });

        request.respond(Response::from_string("OK")).unwrap();

        puzzle_id += 1;
    }

    for mut w in Arc::try_unwrap(workers).unwrap().into_inner().unwrap() {
        w.terminate().unwrap();
    }
    drop(sol_send);
    solution_getter.join().unwrap();
}

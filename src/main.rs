#[macro_use]
extern crate lazy_static;

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::thread;
use structopt::StructOpt;
use tiny_http::{Response, Server};

mod miner;

const WEBHOOK: &'static str = "127.0.0.1:3000";

lazy_static! {
    static ref CURRENT_PUZZLE: Arc<Mutex<Option<RequestWrapper>>> = Arc::new(Mutex::new(None));
}

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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Request {
    key: String,
    blob: String,
    offset: usize,
    size: usize,
    target: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct RequestWrapper {
    puzzle: Option<Request>,
}

fn main() {
    let opt = Opt::from_args();

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
                ureq::post(&format!("{}/miner/solution", opt.node))
                    .send_json(json!({ "nonce": hex::encode(sol.nonce) }))
                    .unwrap();
            }
        })
    };

    let puzzle_getter = thread::spawn(move || loop {
        let pzl = ureq::get(&format!("{}/miner/puzzle", opt.node))
            .call()
            .unwrap()
            .into_string()
            .unwrap();

        let pzl_json: RequestWrapper = serde_json::from_str(&pzl).unwrap();
        if *CURRENT_PUZZLE.lock().unwrap() != Some(pzl_json.clone()) {
            ureq::post(&format!("http://{}", WEBHOOK))
                .send_json(pzl_json)
                .unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    });

    for mut request in server.incoming_requests() {
        // Parse request
        let req: RequestWrapper = {
            let mut content = String::new();
            request.as_reader().read_to_string(&mut content).unwrap();
            serde_json::from_str(&content).unwrap()
        };

        *CURRENT_PUZZLE.lock().unwrap() = Some(req.clone());

        if let Some(req) = req.puzzle {
            println!("Got puzzle... {}", req.target);

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

            request.respond(Response::from_string("\"OK\"")).unwrap();

            puzzle_id += 1;
        } else {
            request.respond(Response::from_string("\"NOK\"")).unwrap();
        }
    }

    for mut w in Arc::try_unwrap(workers).unwrap().into_inner().unwrap() {
        w.terminate().unwrap();
    }
    drop(sol_send);
    solution_getter.join().unwrap();
    puzzle_getter.join().unwrap();
}

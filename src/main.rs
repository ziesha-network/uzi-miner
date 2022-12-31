use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::borrow::Borrow;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use structopt::StructOpt;
use std::time::{Duration, Instant};

use crate::hashrate::Hashrate;

mod miner;
mod hashrate;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "Uzi Miner", about = "Mine Ziesha with Uzi!")]
struct Opt {
    #[structopt(short = "t", long = "threads", default_value = "1")]
    threads: usize,

    #[structopt(short = "r", long = "hashrate", default_value = "1")]
    hashrate: usize,

    #[structopt(short = "n", long = "node")]
    node: SocketAddr,

    #[structopt(long = "slow")]
    slow: bool,

    #[structopt(long = "pool")]
    pool: bool,

    #[structopt(long, default_value = "")]
    miner_token: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Request {
    key: String,
    blob: String,
    offset: usize,
    size: usize,
    target: u32,
    reward: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct RequestWrapper {
    puzzle: Option<Request>,
}

fn process_request(
    context: Arc<Mutex<MinerContext>>,
    req: RequestWrapper,
    opt: &Opt,
    sol_send: std::sync::mpsc::Sender<miner::Solution>,
) -> Result<(), Box<dyn Error>> {
    let mut ctx = context.lock().unwrap();
    ctx.current_puzzle = Some(req.clone());

    if let Some(req) = req.puzzle {
        let power = rust_randomx::Difficulty::new(req.target).power();
        println!(
            "{} Approximately {} hashes need to be calculated...",
            "Got new puzzle!".bright_yellow(),
            power
        );

        // Reinitialize context if needed
        let req_key = hex::decode(&req.key)?;

        if ctx
            .hasher_context
            .as_ref()
            .map(|ctx| ctx.key() != req_key)
            .unwrap_or(true)
        {
            ctx.hasher_context = Some(Arc::new(rust_randomx::Context::new(&req_key, !opt.slow)));
        }

        // Ensure correct number of workers
        while ctx.workers.len() < opt.threads {
            let worker = miner::Worker::new(ctx.worker_id, sol_send.clone());
            ctx.workers.push(worker);
            ctx.worker_id += 1;
        }

        // Send the puzzle to workers
        let hash_ctx = Arc::clone(ctx.hasher_context.as_ref().unwrap());
        let puzzle_id = ctx.puzzle_id;
        let blob = hex::decode(&req.blob)?;
        ctx.workers.retain(|w| {
            w.send(miner::Message::Puzzle(miner::Puzzle {
                id: puzzle_id,
                context: Arc::clone(&hash_ctx),
                blob: blob.clone(),
                offset: req.offset,
                count: req.size,
                target: rust_randomx::Difficulty::new(req.target),
            }))
            .is_ok()
        });

        ctx.puzzle_id += 1;
    } else {
        println!(
            "{} Suspending the workers...",
            "No puzzles available!".bright_yellow()
        );

        // Suspend all workers
        ctx.workers
            .retain(|w| w.send(miner::Message::Break).is_ok());
    }
    Ok(())
}

struct MinerContext {
    hasher_context: Option<Arc<rust_randomx::Context>>,
    current_puzzle: Option<RequestWrapper>,
    workers: Vec<miner::Worker>,
    puzzle_id: u32,
    worker_id: u32,
}

fn main() {
    println!(
        "{} v{} - RandomX CPU Miner for Ziesha Cryptocurrency",
        "Uzi-Miner!".bright_green(),
        env!("CARGO_PKG_VERSION")
    );

    env_logger::init();
    let opt = Opt::from_args();
    let mut nw: usize = 0;

    let (sol_send, sol_recv) = std::sync::mpsc::channel::<miner::Solution>();
    let context = Arc::new(Mutex::new(MinerContext {
        workers: Vec::new(),
        current_puzzle: None,
        hasher_context: None,
        puzzle_id: 0,
        worker_id: 0,
    }));

    let solution_getter = {
        let ctx = Arc::clone(&context);
        let opt = opt.clone();
        thread::spawn(move || {
            let mut _start = Instant::now();
            let mut v: Vec<Option<Hashrate>> = vec![None; opt.threads];
            for sol in sol_recv {
                if opt.hashrate > 0 {
                    let duration: Duration = _start.elapsed();
                    v[sol.hashrate.worker_id as usize] = Some(sol.hashrate.borrow().clone());
                    if ! v.contains(&None) && duration.as_secs_f32() > 30.0 {
                        let mut total: f32 = 0.0;
                        for h in v.iter() {
                            let h2 = h.as_ref().unwrap();
                            if opt.hashrate > 1 {
                                println!("{}", h2);
                            }
                            total += h2.value();
                        }
                        println!("{} = {} {} ({})",
                                 "Total Hashrate".blue(),
                                 format!("{:.3}", total).red(),
                                 "H/s",
                                 format!("{} Workers", v.len()).yellow());
                        _start = Instant::now();
                    }
                }
                if sol.found {
                    if let Err(e) = || -> Result<(), Box<dyn Error>> {
                        println!("{}", "Solution found!".bright_green());
                        if !opt.pool {
                            ctx.lock()?
                                .workers
                                .retain(|w| w.send(miner::Message::Break).is_ok());
                        }
                        ureq::post(&format!("http://{}/miner/solution", opt.node))
                            .set("X-ZIESHA-MINER-TOKEN", &opt.miner_token)
                            .send_json(json!({ "nonce": hex::encode(sol.nonce) }))?;
                        Ok(())
                    }() {
                        log::error!("Error: {}", e);
                    }
                }
            }
        })
    };

    let puzzle_getter = {
        let ctx = Arc::clone(&context);
        let opt = opt.clone();
        let sol_send = sol_send.clone();
        thread::spawn(move || loop {
            if let Err(e) = || -> Result<(), Box<dyn Error>> {
                let pzl = ureq::get(&format!("http://{}/miner/puzzle", opt.node))
                    .set("X-ZIESHA-MINER-TOKEN", &opt.miner_token)
                    .call()?
                    .into_string()?;

                let pzl_json: RequestWrapper = serde_json::from_str(&pzl)?;

                if ctx.lock()?.current_puzzle != Some(pzl_json.clone()) {
                    process_request(ctx.clone(), pzl_json, &opt, sol_send.clone())?;
                    nw = ctx.lock()?.workers.len();
                    log::info!("nWorkers: {}", nw);
                }
                Ok(())
            }() {
                log::error!("Error: {}", e);
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        })
    };

    if let Ok(ctx) = Arc::try_unwrap(context) {
        for mut w in ctx.into_inner().unwrap().workers {
            w.terminate().unwrap();
        }
    }
    drop(sol_send);
    solution_getter.join().unwrap();
    puzzle_getter.join().unwrap();
}

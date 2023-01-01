use crate::hashrate::Hashrate;
use rand::prelude::*;
use rust_randomx::{Context, Difficulty, Hasher};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("message send error")]
    MessageSendError(#[from] mpsc::SendError<Message>),
    #[error("solution send error")]
    SolutionSendError(#[from] mpsc::SendError<Solution>),
    #[error("recv error")]
    RecvError(#[from] mpsc::RecvError),
    #[error("worker is terminated")]
    Terminated,
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub id: u32,
    pub nonce: Vec<u8>,
}

unsafe impl Send for Solution {}
unsafe impl Sync for Solution {}

#[derive(Clone)]
pub struct Puzzle {
    pub id: u32,
    pub context: Arc<Context>,
    pub blob: Vec<u8>,
    pub offset: usize,
    pub count: usize,
    pub target: Difficulty,
}

#[derive(Clone)]
pub enum Message {
    Puzzle(Puzzle),
    Break,
    Terminate,
}

unsafe impl Send for Puzzle {}
unsafe impl Sync for Puzzle {}

#[derive(Debug)]
pub struct Worker {
    worker_id: u32,
    handle: Option<thread::JoinHandle<Result<(), WorkerError>>>,
    chan: mpsc::Sender<Message>,
}

const HASH_PER_ROUND: usize = 512;

impl Worker {
    pub fn send(&self, msg: Message) -> Result<(), WorkerError> {
        if self.handle.is_some() {
            self.chan.send(msg)?;
            Ok(())
        } else {
            Err(WorkerError::Terminated)
        }
    }
    pub fn terminate(&mut self) -> Result<(), WorkerError> {
        if let Some(handle) = self.handle.take() {
            log::info!("Terminating worker {}...", self.worker_id);
            self.chan.send(Message::Terminate)?;
            handle.join().unwrap()
        } else {
            Err(WorkerError::Terminated)
        }
    }
    pub fn new(
        worker_id: u32,
        callback: mpsc::Sender<Solution>,
        hashrate_sender: mpsc::Sender<(u32, Hashrate)>,
    ) -> Self {
        let (msg_send, msg_recv) = mpsc::channel::<Message>();
        let handle = thread::spawn(move || -> Result<(), WorkerError> {
            let mut rng = rand::thread_rng();
            let mut msg = msg_recv.recv()?;

            loop {
                let mut puzzle = match msg.clone() {
                    Message::Puzzle(puzzle) => puzzle,
                    Message::Break => {
                        msg = msg_recv.recv()?;
                        continue;
                    }
                    Message::Terminate => {
                        log::info!("Worker {} terminated!", worker_id);
                        return Ok(());
                    }
                };

                let mut counter = 0;

                let mut hasher = Hasher::new(Arc::clone(&puzzle.context));

                let (b, e) = (puzzle.offset, puzzle.offset + puzzle.count);

                rng.fill_bytes(&mut puzzle.blob[b..e]);
                hasher.hash_first(&puzzle.blob);

                let mut start = Instant::now();
                loop {
                    let prev_nonce = puzzle.blob[b..e].to_vec();

                    rng.fill_bytes(&mut puzzle.blob[b..e]);
                    let out = hasher.hash_next(&puzzle.blob);

                    if out.meets_difficulty(puzzle.target) {
                        callback.send(Solution {
                            id: puzzle.id,
                            nonce: prev_nonce,
                        })?;
                    }
                    counter += 1;

                    // Every HASH_PER_ROUND hashes, if there is a new message, cancel the current
                    // puzzle and process the message.
                    if counter >= HASH_PER_ROUND {
                        if let Ok(new_msg) = msg_recv.try_recv() {
                            msg = new_msg;
                            break;
                        }
                        let elapsed = start.elapsed().as_millis() as f32 / 1000.0;
                        hashrate_sender
                            .send((worker_id, Hashrate(HASH_PER_ROUND as f32 / elapsed)))
                            .unwrap_or_else(|err| println!("{:?}", err));
                        start = Instant::now();
                        counter = 0;
                    }
                }
                hasher.hash_last();
            }
        });
        log::info!("Worker {} created!", worker_id);

        Self {
            worker_id,
            handle: Some(handle),
            chan: msg_send,
        }
    }
}

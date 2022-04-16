use rand::prelude::*;
use rust_randomx::{Context, Difficulty, Hasher};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
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
    handle: Option<thread::JoinHandle<Result<(), WorkerError>>>,
    chan: mpsc::Sender<Message>,
}

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
            println!("Terminating...");
            if self.chan.send(Message::Terminate).is_err() {
                println!("Channel broken!");
            }
            handle.join().unwrap()
        } else {
            Err(WorkerError::Terminated)
        }
    }
    pub fn new(callback: mpsc::Sender<Solution>) -> Self {
        let (msg_send, msg_recv) = mpsc::channel::<Message>();
        let handle = thread::spawn(move || -> Result<(), WorkerError> {
            let mut rng = rand::thread_rng();
            let mut msg = msg_recv.recv()?;

            loop {
                let mut puzzle = match msg.clone() {
                    Message::Puzzle(puzzle) => puzzle,
                    Message::Break => {
                        println!("Mining interrupted!");
                        msg = msg_recv.recv()?;
                        continue;
                    }
                    Message::Terminate => {
                        return Ok(());
                    }
                };
                let mut hasher = Hasher::new(Arc::clone(&puzzle.context));
                let mut nonce: u32 = rng.gen();
                let mut counter = 0;
                puzzle.blob[puzzle.offset..puzzle.count].copy_from_slice(&nonce.to_le_bytes());
                hasher.hash_first(&puzzle.blob);

                loop {
                    let next_nonce: u32 = rng.gen();
                    puzzle.blob[puzzle.offset..puzzle.count]
                        .copy_from_slice(&next_nonce.to_le_bytes());
                    let out = hasher.hash_next(&puzzle.blob);
                    if out.meets_difficulty(puzzle.target) {
                        callback.send(Solution {
                            id: puzzle.id,
                            nonce: nonce.to_le_bytes().to_vec(),
                        })?;
                    }
                    nonce = next_nonce;
                    counter += 1;

                    // Every 4096 hashes, if there is a new message, cancel the current
                    // puzzle and process the message.
                    if counter >= 4096 {
                        if let Ok(new_msg) = msg_recv.try_recv() {
                            msg = new_msg;
                            break;
                        }
                        counter = 0;
                    }
                }

                hasher.hash_last();
            }
        });

        Self {
            handle: Some(handle),
            chan: msg_send,
        }
    }
}

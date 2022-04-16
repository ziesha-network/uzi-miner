use rand::prelude::*;
use rust_randomx::{Context, Difficulty, Hasher};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

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
    pub handle: thread::JoinHandle<()>,
    pub chan: mpsc::Sender<Message>,
}

impl Worker {
    pub fn new(callback: mpsc::Sender<Solution>) -> Self {
        let (msg_send, msg_recv) = mpsc::channel::<Message>();
        let handle = thread::spawn(move || {
            let mut rng = rand::thread_rng();
            loop {
                let mut puzzle = match msg_recv.recv().unwrap() {
                    Message::Puzzle(puzzle) => puzzle,
                    Message::Break => {
                        continue;
                    }
                    Message::Terminate => {
                        return;
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
                        if callback
                            .send(Solution {
                                id: puzzle.id,
                                nonce: nonce.to_le_bytes().to_vec(),
                            })
                            .is_err()
                        {
                            println!("Puzzle callback failed!");
                        }
                        hasher.hash_last();
                        break;
                    }
                    nonce = next_nonce;
                    counter += 1;

                    // Every 4096 hashes, if there is a new puzzle, switch to the
                    // new puzzle.
                    if counter >= 4096 {
                        if let Ok(new_msg) = msg_recv.try_recv() {
                            match new_msg {
                                Message::Puzzle(new_puzzle) => {
                                    hasher.hash_last();
                                    puzzle = new_puzzle;
                                    hasher = Hasher::new(Arc::clone(&puzzle.context));
                                    hasher.hash_first(&nonce.to_le_bytes());
                                    counter = 0;
                                }
                                Message::Break => {
                                    break;
                                }
                                Message::Terminate => {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        });
        Self {
            handle,
            chan: msg_send,
        }
    }
}

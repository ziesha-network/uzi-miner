use rand::prelude::*;
use rust_randomx::{Context, Difficulty, Hasher};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

#[derive(Clone)]
pub struct Solution {
    pub nonce: Vec<u8>,
}

unsafe impl Send for Solution {}
unsafe impl Sync for Solution {}

#[derive(Clone)]
pub struct Puzzle {
    pub context: Arc<Context>,
    pub blob: Vec<u8>,
    pub offset: usize,
    pub count: usize,
    pub target: Difficulty,
    pub callback: mpsc::Sender<Solution>,
}

unsafe impl Send for Puzzle {}
unsafe impl Sync for Puzzle {}

pub fn new_worker() -> (thread::JoinHandle<()>, mpsc::Sender<Puzzle>) {
    let (puzzle_send, puzzle_recv) = mpsc::channel::<Puzzle>();
    let handle = thread::spawn(move || {
        let mut rng = rand::thread_rng();
        loop {
            let mut puzzle = puzzle_recv.recv().unwrap();
            let mut hasher = Hasher::new(Arc::clone(&puzzle.context));
            let mut nonce: u32 = rng.gen();
            let mut counter = 0;
            hasher.hash_first(&nonce.to_le_bytes());

            loop {
                let next_nonce: u32 = rng.gen();
                let out = hasher.hash_next(&next_nonce.to_le_bytes());
                if out.meets_difficulty(puzzle.target) {
                    if puzzle
                        .callback
                        .send(Solution {
                            nonce: nonce.to_le_bytes().to_vec(),
                        })
                        .is_err()
                    {
                        println!("Puzzle callback failed!");
                    }
                    break;
                }
                nonce = next_nonce;
                counter += 1;

                // Every 4096 hashes, if there is a new puzzle, switch to the
                // new puzzle.
                if counter >= 4096 {
                    if let Ok(new_puzzle) = puzzle_recv.try_recv() {
                        puzzle = new_puzzle;
                        hasher = Hasher::new(Arc::clone(&puzzle.context));
                        hasher.hash_first(&nonce.to_le_bytes());
                        counter = 0;
                    }
                }
            }
        }
    });
    (handle, puzzle_send)
}

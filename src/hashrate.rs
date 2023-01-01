use std::time::{Duration, Instant};
use colored::*;


pub fn get_unit(hashrate: f32) -> (f32, String) {
    let mut hashrate = hashrate as f32;
    let mut unit = "";
    let units = vec!["k", "M", "G", "T", "P", "E", "Z", "Y"];
    for u in units.iter() {
        if hashrate > 1000.0 {
            hashrate /= 1000.0;
            unit = u;
        } else {
            break;
        }
    }
    (hashrate, format!("{}H/s", unit))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hashrate {
    pub worker_id: u32,
    value: f32,
    counter: u32,
    counter_max: u32,
    sampling_start: Instant,
    report_start: Instant,
    report_interval: Duration,
}

impl std::fmt::Display for Hashrate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (value, unit) = get_unit(self.value);
        write!(f, "Worker {} Hashrate = {} {}",
               format!("#{}", self.worker_id).yellow(),
               format!("{:.3}", value).red(),
               unit)
    }
}

impl core::iter::Sum for Hashrate {
    fn sum<I: Iterator<Item=Hashrate>>(iter: I) -> Hashrate {
        iter.fold(Hashrate::new(0, 0.0), |a, b| Hashrate::new(0, a.value + b.value))
    }
}

impl Hashrate {
    pub fn new(worker_id: u32, value: f32) -> Self {
        Self { worker_id, value, counter: 0, 
               counter_max: 32,
               sampling_start: Instant::now(),
               report_start: Instant::now(),
               report_interval: Duration::from_secs(60)}
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    pub fn available(&self) -> bool {
        self.report_start.elapsed() > self.report_interval
    }
    pub fn count(&mut self) {
        self.counter += 1;
        // Every counter_max hashes, calculate the hashrate and reset the timer.
        if self.counter >= self.counter_max {
            let duration: f32 = self.sampling_start.elapsed()
                .as_millis() as f32;
            let duration: f32 = duration/ 1_000.0;
            let hashrate: f32 = self.counter as f32 / duration;
            self.value = (self.value + hashrate) / 2.0;
            self.sampling_start = Instant::now();
            self.counter = 0;
        }
    }
}
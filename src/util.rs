use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::time::Instant;

use parking_lot::Mutex;

use std::io::{stdout, Write};

pub struct ProgressInfo {
    current_bytes: AtomicU64,
    target_bytes: AtomicU64,
    last_printed: Mutex<Instant>,
}

impl ProgressInfo {
    pub fn new(target_size_mb: f64) -> Self {
        Self {
            current_bytes: AtomicU64::new(0),
            target_bytes: AtomicU64::new((target_size_mb * 1024.0 * 1024.0) as u64),
            last_printed: Mutex::new(std::time::Instant::now()),
        }
    }

    pub fn update(&self, chunk_size: usize) {
        self.current_bytes
            .fetch_add(chunk_size as u64, Ordering::Relaxed);
    }

    pub fn print_progress(&self) {
        let mut last_printed = self.last_printed.lock();
        let now = std::time::Instant::now();
        if now.duration_since(*last_printed).as_millis() < 50 {
            return;
        }
        *last_printed = now;

        let current = self.current_bytes.load(Ordering::Relaxed) as f64;
        let target = self.target_bytes.load(Ordering::Relaxed) as f64;
        let percentage = (current / target).min(1.0);
        let filled = ((percentage * 50.0) as usize).min(50);
        let remaining = 50 - filled;

        print!("\x1B[6;1H\r\x1B[K");

        let current_mb = current / (1024.0 * 1024.0);
        let target_mb = target / (1024.0 * 1024.0);

        print!(
            "Generating data: [{}{}] {:.2}MB/{:.2}MB ({:.2}%)",
            "=".repeat(filled),
            ".".repeat(remaining),
            current_mb,
            target_mb,
            percentage * 100.0
        );
        stdout().flush().unwrap();

        if current_mb >= target_mb {
            println!("\nGeneration complete!");
        }
    }
}

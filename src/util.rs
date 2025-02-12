use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::time::Instant;

use parking_lot::Mutex;

use std::io::{stdout, Write};

use crossterm::{
    cursor,
    style::Print,
    terminal::{Clear, ClearType},
    QueueableCommand,
};

const BYTE_SIZE: f64 = 1024.0 * 1024.0;
pub struct ProgressInfo {
    current_bytes: AtomicU64,
    target_bytes: AtomicU64,
    streamed_bytes: AtomicU64,
    last_printed: Mutex<Instant>,
}

impl ProgressInfo {
    pub fn new(target_size_mb: u64) -> Self {
        Self {
            current_bytes: AtomicU64::new(0),
            target_bytes: AtomicU64::new(target_size_mb),
            streamed_bytes: AtomicU64::new(0),
            last_printed: Mutex::new(std::time::Instant::now()),
        }
    }

    pub fn update(&self, chunk_size: usize) {
        self.current_bytes
            .fetch_add(chunk_size as u64, Ordering::Relaxed);
    }

    pub fn update_streamed(&self, chunk_size: usize) {
        self.streamed_bytes
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
        let streamed = self.streamed_bytes.load(Ordering::Relaxed) as f64;
        let target = self.target_bytes.load(Ordering::Relaxed) as f64;

        let gen_percentage = (current / target).min(1.0);
        let stream_percentage = (streamed / target).min(1.0);

        let gen_filled = ((gen_percentage * 50.0) as usize).min(50);
        let stream_filled = ((stream_percentage * 50.0) as usize).min(50);

        let current_mb = current / BYTE_SIZE;
        let streamed_mb = streamed / BYTE_SIZE;
        let target_mb = target / BYTE_SIZE;

        let mut stdout = stdout();

        // Generate the progress bar strings
        let gen_line = format!(
            "Generating data: [{}{}] {:.2}MB/{:.2}MB ({:.2}%)",
            "=".repeat(gen_filled),
            ".".repeat(50 - gen_filled),
            current_mb,
            target_mb,
            gen_percentage * 100.0
        );

        let stream_line = format!(
            "Streaming data:  [{}{}] {:.2}MB/{:.2}MB ({:.2}%)",
            "=".repeat(stream_filled),
            ".".repeat(50 - stream_filled),
            streamed_mb,
            target_mb,
            stream_percentage * 100.0
        );

        stdout
            .queue(cursor::SavePosition)
            .unwrap()
            .queue(cursor::MoveTo(0, 5))
            .unwrap()
            .queue(Clear(ClearType::FromCursorDown))
            .unwrap()
            .queue(Print(&gen_line))
            .unwrap()
            .queue(cursor::MoveToNextLine(1))
            .unwrap()
            .queue(Print(&stream_line))
            .unwrap();

        if current_mb >= target_mb && streamed_mb >= target_mb {
            stdout
                .queue(cursor::MoveToNextLine(1))
                .unwrap() // Move one more line down
                .queue(Clear(ClearType::CurrentLine))
                .unwrap() // Clear the line
                .queue(Print("Generation and streaming complete!\n"))
                .unwrap();
        }

        stdout
            .queue(cursor::RestorePosition)
            .unwrap()
            .flush()
            .unwrap();
    }
}

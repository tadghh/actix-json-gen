use crate::processing::OutputFormat;
use anyhow::{Context, Result};
use core::sync::atomic::{AtomicU64, Ordering};
use crossterm::{
    cursor,
    style::Print,
    terminal::{Clear, ClearType},
    QueueableCommand,
};
use parking_lot::Mutex;
use std::io::{stdout, Write};
use std::time::Instant;
pub struct ProgressInfo {
    current_bytes: AtomicU64,
    target_bytes: u64,
    streamed_bytes: AtomicU64,
    last_printed: Mutex<Instant>,
    byte_size: u64,
    format: String,
}

impl ProgressInfo {
    pub fn new(target_size_bytes: u64, factor: u64, format: String) -> Self {
        Self {
            current_bytes: AtomicU64::new(0),
            target_bytes: target_size_bytes,
            streamed_bytes: AtomicU64::new(0),
            last_printed: Mutex::new(Instant::now()),
            byte_size: factor,
            format: format.to_uppercase(),
        }
    }

    pub fn total_size(&self) -> u64 {
        self.target_bytes
    }

    pub fn update(&self, chunk_size: usize) {
        self.current_bytes
            .fetch_add(chunk_size as u64, Ordering::Relaxed);
    }

    pub fn update_streamed(&self, chunk_size: usize) {
        self.streamed_bytes
            .fetch_add(chunk_size as u64, Ordering::Relaxed);
    }

    pub fn print_header(&self, type_item: OutputFormat) {
        print!("\x1B[2J\x1B[1;1H");
        println!("Starting new streaming data generation request:");
        println!("------------------------------------");
        println!(
            "Requested size: {}{}",
            self.target_bytes / self.byte_size,
            self.format
        );
        println!("Format: {}", type_item.to_string());
    }

    pub fn print_progress(&self) {
        let mut last_printed = self.last_printed.lock();
        let now = Instant::now();
        if now.duration_since(*last_printed).as_millis() < 252 {
            return;
        }
        *last_printed = now;

        let current = self.current_bytes.load(Ordering::Relaxed) as f64;
        let streamed = self.streamed_bytes.load(Ordering::Relaxed) as f64;
        let target = self.target_bytes as f64;

        let gen_percentage = (current / target).min(1.0);
        let stream_percentage = (streamed / target).min(1.0);

        let gen_filled = ((gen_percentage * 50.0) as usize).min(50);
        let stream_filled = ((stream_percentage * 50.0) as usize).min(50);

        let current_mb = current / self.byte_size as f64;
        let streamed_mb = streamed / self.byte_size as f64;
        let target_mb = target / self.byte_size as f64;

        let mut stdout = stdout();
        let format = self.format.clone();

        let gen_line = format!(
            "Generating data: [{}{}] {:.2}{format}/{:.2}{format} ({:.2}%)",
            "=".repeat(gen_filled),
            ".".repeat(50 - gen_filled),
            current_mb,
            target_mb,
            gen_percentage * 100.0
        );

        let stream_line = format!(
            "Streaming data:  [{}{}] {:.2}{format}/{:.2}{format} ({:.2}%)",
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

        if streamed_mb >= target_mb {
            stdout
                .queue(cursor::MoveToNextLine(1))
                .unwrap()
                .queue(Clear(ClearType::CurrentLine))
                .unwrap()
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

#[derive(Debug)]
pub struct SizeInfo {
    pub total_size: u64,
    pub multiplier: u64,
    pub unit: String,
}

pub fn parse_size(size_str: &str) -> Result<SizeInfo, String> {
    let size_str = size_str.to_lowercase();
    let (number_str, unit) = size_str
        .find(|c: char| !c.is_digit(10))
        .map(|i| size_str.split_at(i))
        .ok_or_else(|| "Invalid format".to_string())?;

    let number: u64 = number_str
        .parse()
        .map_err(|_| "Invalid number".to_string())?;

    let multiplier = match unit {
        "b" => 8,
        "kb" => 1024_u64.pow(1),
        "mb" => 1024_u64.pow(2),
        "gb" => 1024_u64.pow(3),
        "tb" => 1024_u64.pow(4),
        // "pb" => 1024_u64.pow(5),
        // "eb" => 1024_u64.pow(6),
        // "zb" => 1024_u64.pow(7),
        // "yb" => 1024_u64.pow(8),
        _ => return Err("Invalid unit".to_string()),
    };

    Ok(SizeInfo {
        total_size: number * multiplier,
        multiplier,
        unit: unit.to_owned(),
    })
}

pub fn get_size_info(size: Option<&String>) -> Result<SizeInfo> {
    match size {
        Some(size_str) => parse_size(size_str)
            .map_err(|e| anyhow::anyhow!(e))
            .context("Failed to parse size"),
        None => Err(anyhow::anyhow!("Size parameter is missing")),
    }
}

pub fn convert_error(err: anyhow::Error) -> actix_web::Error {
    actix_web::error::ErrorBadRequest(err.to_string())
}

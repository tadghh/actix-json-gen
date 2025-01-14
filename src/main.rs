#![feature(get_mut_unchecked, portable_simd)]
use actix_web::{web, App, Error, HttpResponse, HttpServer};

use parking_lot::Mutex;
use processing::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use std::collections::HashMap;
use std::io::{stdout, Write};

use std::sync::Arc;
mod processing;

fn parse_size(size_str: &str) -> Result<usize, String> {
    let size_str = size_str.to_lowercase();
    let (number_str, unit) = size_str
        .find(|c: char| !c.is_digit(10))
        .map(|i| size_str.split_at(i))
        .ok_or_else(|| "Invalid format".to_string())?;

    let number: usize = number_str
        .parse()
        .map_err(|_| "Invalid number".to_string())?;

    let multiplier = match unit {
        "b" => 1,
        "kb" => 1024,
        "mb" => 1024 * 1024,
        "gb" => 1024 * 1024 * 1024,
        _ => return Err("Invalid unit".to_string()),
    };

    Ok(number * multiplier)
}

async fn generate_data(
    web::Query(params): web::Query<HashMap<String, String>>,
    data_pools: web::Data<Arc<DataPools>>,
) -> Result<HttpResponse, Error> {
    const BYTE_SIZE: usize = 1024 * 1024;

    let target_size = if let Some(size_str) = params.get("size") {
        parse_size(size_str).unwrap_or(BYTE_SIZE)
    } else {
        BYTE_SIZE
    };

    let pretty = params.get("pretty").map_or(false, |v| v == "true");
    let format = OutputFormat::from_str(params.get("format").map_or("json", |s| s));
    let seed: u64 = rand::thread_rng().gen();

    let num_threads = num_cpus::get();
    let chunk_size = target_size / num_threads;

    let total_generated = Arc::new(Mutex::new(0usize));

    let progress = Arc::new(Mutex::new(ProgressInfo::new(
        (target_size / (BYTE_SIZE)) as f64,
    )));

    print!("\x1B[2J\x1B[1;1H");
    println!("Starting new data generation request:");
    println!("------------------------------------");
    println!("Requested size: {:.2}MB", target_size / (BYTE_SIZE));
    println!(
        "Format: {}",
        if format == OutputFormat::JSON {
            "JSON"
        } else {
            "CSV"
        }
    );
    if format == OutputFormat::JSON {
        println!(
            "Pretty print: {}",
            params.get("pretty").map_or("false", |v| v)
        );
    }

    let chunks: Vec<ChunkResult> = (0..num_threads)
        .into_par_iter()
        .map(|i| {
            let is_first = i == 0;
            let chunk_rng = ChaCha8Rng::seed_from_u64(seed.wrapping_add(i as u64));
            let start_id = *total_generated.lock();

            let result = generate_chunk(
                start_id as u32,
                chunk_size,
                &data_pools,
                chunk_rng,
                pretty,
                is_first,
                &format,
                progress.clone(),
            );
            *total_generated.lock() += result.count;

            result
        })
        .collect();

    println!("\nCombining Data.");
    let mut output = Vec::with_capacity(target_size + 1024);

    for chunk in chunks {
        output.extend(chunk.data);
    }

    if format == OutputFormat::JSON {
        output.extend_from_slice(if pretty { b"\n]\n" } else { b"]" });
    }

    println!("Done.");
    progress.lock().print_progress();

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", format.content_type()))
        .body(output))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Starting server at http://127.0.0.1:8080");
    println!("Using {} CPU cores for data generation", num_cpus::get());

    let data_pools = Arc::new(DataPools::new());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(data_pools.clone()))
            .route("/generate", web::get().to(generate_data))
    })
    .bind("127.0.0.1:8080")?
    .workers(num_cpus::get())
    .run()
    .await
}
struct ProgressInfo {
    current_size: f64,
    target_size: f64,
    last_printed: std::time::Instant,
}

impl ProgressInfo {
    const SIZES: f64 = 1024.0 * 1024.0;
    fn new(target_size: f64) -> Self {
        Self {
            current_size: 0.0,
            target_size,
            last_printed: std::time::Instant::now(),
        }
    }

    fn update(&mut self, chunk_size: usize) {
        let new_size =
            ((self.current_size + chunk_size as f64) / Self::SIZES) * num_cpus::get() as f64;

        // Make sure we dont regress 😩
        if self.current_size < new_size {
            self.current_size = new_size;
        }
    }

    fn print_progress(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_printed).as_millis() < 50 {
            return;
        }
        self.last_printed = now;

        let percentage = self.current_size / self.target_size;
        let bar_width = 50.0;

        let filled = (percentage * bar_width).round() as usize;
        let remaining = (bar_width - filled as f64).round() as usize;

        // Move cursor to line 6 (after the headers)
        print!("\x1B[6;1H");

        // Clear line and move to start
        print!("\r\x1B[K");

        let current_mb = self.current_size;
        let target_mb = self.target_size;

        print!(
            "Generating data: [{}{}] {:.2}MB/{:.2}MB ({:.2}%)",
            "=".repeat(filled),
            ".".repeat(remaining),
            current_mb,
            target_mb,
            percentage * 100.0
        );
        stdout().flush().unwrap();

        if self.current_size >= self.target_size {
            println!("\nGeneration complete!");
        }
    }
}

#![feature(get_mut_unchecked)]

use actix_web::{web, App, Error, HttpResponse, HttpServer};
use fake::faker::address::en::*;
use fake::faker::company::en::*;
use fake::Fake;
use parking_lot::Mutex;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{stdout, Write};

use std::sync::Arc;

#[derive(Serialize)]
struct BusinessLocation {
    id: u32,
    name: String,
    industry: String,
    revenue: f32,
    employees: u32,
    city: String,
    state: String,
    country: String,
}

struct DataPools {
    names: Vec<String>,
    industries: Vec<String>,
    cities: Vec<String>,
    states: Vec<String>,
    countries: Vec<String>,
}

struct ChunkResult {
    data: Vec<u8>,
    count: usize,
}

impl DataPools {
    fn new() -> Self {
        let pool_size = 1000;
        DataPools {
            names: (0..pool_size).map(|_| CompanyName().fake()).collect(),
            industries: (0..pool_size).map(|_| Industry().fake()).collect(),
            cities: (0..pool_size).map(|_| CityName().fake()).collect(),
            states: (0..pool_size).map(|_| StateName().fake()).collect(),
            countries: (0..50).map(|_| CountryName().fake()).collect(),
        }
    }
}

#[inline]
fn generate_chunk(
    start_id: u32,
    target_chunk_size: usize,
    pools: &DataPools,
    mut rng: ChaCha8Rng,
    pretty: bool,
    is_first: bool,
    mut progress: Arc<Mutex<ProgressInfo>>,
) -> ChunkResult {
    let mut output = Vec::with_capacity(target_chunk_size + 1024);
    if is_first {
        output.extend_from_slice(if pretty { b"[\n  " } else { b"[" });
    }

    let mut count = 0;
    let mut current_id = start_id;
    let separa = if pretty {
        ",\n    ".to_string()
    } else {
        ",".to_string()
    };
    let separator = separa.as_bytes();
    let line_en: String = if pretty {
        "\n  }".to_string()
    } else {
        "}".to_string()
    };
    let line_end = line_en.as_bytes();
    while output.len() < target_chunk_size {
        if count > 0 || !is_first {
            output.extend_from_slice(if pretty { b",\n  " } else { b"," });
        }

        let random_number = rng.gen_range(0..100);
        let location = BusinessLocation {
            id: current_id,
            name: pools.names[random_number].clone(),
            industry: pools.industries[random_number].clone(),
            revenue: rng.gen_range(100000.0..100000000.0),
            employees: rng.gen_range(10..10000),
            city: pools.cities[random_number].clone(),
            state: pools.states[random_number].clone(),
            country: pools.countries[rng.gen_range(0..5)].clone(),
        };

        output.extend_from_slice(b"{");
        if pretty {
            output.extend_from_slice(b"\n    ");
        }

        output.extend_from_slice(b"\"id\": ");
        output.extend_from_slice(location.id.to_string().as_bytes());

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"name\": \"");
        output.extend_from_slice(location.name.as_bytes());
        output.extend_from_slice(b"\"");

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"industry\": \"");
        output.extend_from_slice(location.industry.as_bytes());
        output.extend_from_slice(b"\"");

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"revenue\": ");
        output.extend_from_slice(location.revenue.to_string().as_bytes());

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"employees\": ");
        output.extend_from_slice(location.employees.to_string().as_bytes());

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"city\": \"");
        output.extend_from_slice(location.city.as_bytes());
        output.extend_from_slice(b"\"");

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"state\": \"");
        output.extend_from_slice(location.state.as_bytes());
        output.extend_from_slice(b"\"");

        output.extend_from_slice(separator);
        output.extend_from_slice(b"\"country\": \"");
        output.extend_from_slice(location.country.as_bytes());
        output.extend_from_slice(b"\"");

        output.extend_from_slice(line_end);

        // It'll be fine.
        unsafe {
            let progress_locked = Arc::get_mut_unchecked(&mut progress);

            // yay
            progress_locked.force_unlock();
            progress_locked.get_mut().update(output.len());
        }

        // Numbers only go up!
        if count % 1500 == 0 {
            progress.lock().print_progress();
        }

        count += 1;
        current_id += 1;
    }

    ChunkResult {
        data: output,
        count,
    }
}

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
        "Pretty print: {}",
        params.get("pretty").map_or("false", |v| v)
    );

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
                progress.clone(),
            );
            *total_generated.lock() += result.count;

            result
        })
        .collect();

    println!("Combinding Data.");
    let mut output = Vec::with_capacity(target_size + 1024);

    for chunk in chunks {
        output.extend(chunk.data);
    }
    output.extend_from_slice(if pretty { b"\n]\n" } else { b"]" });

    println!("Done.");

    progress.lock().print_progress();
    Ok(HttpResponse::Ok()
        .insert_header(("Content-Encoding", "gzip"))
        .insert_header(("Content-Type", "application/json"))
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

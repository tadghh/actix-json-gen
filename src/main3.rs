#![feature(get_mut_unchecked)]

use actix_web::{web, App, Error, HttpResponse, HttpServer};
use fake::faker::address::en::*;
use fake::faker::company::en::*;
use fake::Fake;
use parking_lot::Mutex;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{stdout, Write};
use std::sync::Arc;

// Constants for JSON formatting
const COMMA: &[u8] = b",";
const QUOTE: &[u8] = b"\"";
const COLON: &[u8] = b": ";
const OPEN_BRACE: &[u8] = b"{";
const CLOSE_BRACE: &[u8] = b"}";
const OPEN_BRACKET: &[u8] = b"[";
const CLOSE_BRACKET: &[u8] = b"]";
const NEWLINE: &[u8] = b"\n";
const SPACE: &[u8] = b"    ";

// Business location structure without Serialize derive
struct BusinessLocation {
    id: u32,
    name: Arc<String>,
    industry: Arc<String>,
    revenue: f32,
    employees: u32,
    city: Arc<String>,
    state: Arc<String>,
    country: Arc<String>,
}

impl BusinessLocation {
    fn write_to_buffer(&self, buffer: &mut Vec<u8>, pretty: bool, separator: &[u8]) {
        buffer.extend_from_slice(OPEN_BRACE);
        if pretty {
            buffer.extend_from_slice(NEWLINE);
            buffer.extend_from_slice(SPACE);
        }

        // Write ID
        buffer.extend_from_slice(b"\"id\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(self.id.to_string().as_bytes());

        // Write Name
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"name\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(QUOTE);
        buffer.extend_from_slice(self.name.as_bytes());
        buffer.extend_from_slice(QUOTE);

        // Write Industry
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"industry\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(QUOTE);
        buffer.extend_from_slice(self.industry.as_bytes());
        buffer.extend_from_slice(QUOTE);

        // Write Revenue
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"revenue\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(self.revenue.to_string().as_bytes());

        // Write Employees
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"employees\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(self.employees.to_string().as_bytes());

        // Write City
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"city\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(QUOTE);
        buffer.extend_from_slice(self.city.as_bytes());
        buffer.extend_from_slice(QUOTE);

        // Write State
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"state\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(QUOTE);
        buffer.extend_from_slice(self.state.as_bytes());
        buffer.extend_from_slice(QUOTE);

        // Write Country
        buffer.extend_from_slice(separator);
        buffer.extend_from_slice(b"\"country\"");
        buffer.extend_from_slice(COLON);
        buffer.extend_from_slice(QUOTE);
        buffer.extend_from_slice(self.country.as_bytes());
        buffer.extend_from_slice(QUOTE);

        if pretty {
            buffer.extend_from_slice(NEWLINE);
            buffer.extend_from_slice(SPACE);
        }
        buffer.extend_from_slice(CLOSE_BRACE);
    }
}

// Optimized data pools using Arc<String>
struct DataPools {
    names: Vec<Arc<String>>,
    industries: Vec<Arc<String>>,
    cities: Vec<Arc<String>>,
    states: Vec<Arc<String>>,
    countries: Vec<Arc<String>>,
    buffer_pool: Mutex<BufferPool>,
    rng_pool: Mutex<RandomPool>,
}

// Buffer pooling implementation
struct BufferPool {
    buffers: Vec<Vec<u8>>,
}

impl BufferPool {
    fn new() -> Self {
        Self {
            buffers: Vec::with_capacity(num_cpus::get()),
        }
    }

    fn get_buffer(&mut self, capacity: usize) -> Vec<u8> {
        self.buffers
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(capacity))
    }

    fn return_buffer(&mut self, mut buffer: Vec<u8>) {
        buffer.clear();
        self.buffers.push(buffer);
    }
}

// Random number pooling
struct RandomPool {
    numbers: Vec<u32>,
    index: usize,
}

impl RandomPool {
    fn new(size: usize, rng: &mut ChaCha8Rng) -> Self {
        let numbers: Vec<u32> = (0..size).map(|_| rng.gen()).collect();
        Self { numbers, index: 0 }
    }

    fn next(&mut self) -> u32 {
        let value = self.numbers[self.index];
        self.index = (self.index + 1) % self.numbers.len();
        value
    }
}

impl DataPools {
    fn new() -> Self {
        let pool_size = 1000;
        let mut rng = ChaCha8Rng::seed_from_u64(0);

        Self {
            names: (0..pool_size)
                .map(|_| Arc::new(CompanyName().fake()))
                .collect(),
            industries: (0..pool_size)
                .map(|_| Arc::new(Industry().fake()))
                .collect(),
            cities: (0..pool_size)
                .map(|_| Arc::new(CityName().fake()))
                .collect(),
            states: (0..pool_size)
                .map(|_| Arc::new(StateName().fake()))
                .collect(),
            countries: (0..50).map(|_| Arc::new(CountryName().fake())).collect(),
            buffer_pool: Mutex::new(BufferPool::new()),
            rng_pool: Mutex::new(RandomPool::new(1000, &mut rng)),
        }
    }
}

struct ChunkResult {
    data: Vec<u8>,
    count: usize,
}

impl ChunkResult {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            count: 0,
        }
    }
}

// Optimized chunk generation
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
    let mut result = ChunkResult::with_capacity(target_chunk_size + 1024);
    let mut buffer = &mut result.data;
    let mut rng_pool = pools.rng_pool.lock();

    if is_first {
        buffer.extend_from_slice(OPEN_BRACKET);
        if pretty {
            buffer.extend_from_slice(NEWLINE);
            buffer.extend_from_slice(SPACE);
        }
    }

    let separator = if pretty { b",\n    " } else { b",     " };

    let mut count = 0;
    let mut current_id = start_id;

    while buffer.len() < target_chunk_size {
        if count > 0 || !is_first {
            buffer.extend_from_slice(if pretty { b",\n  " } else { COMMA });
        }

        let random_number = (rng_pool.next() % 100) as usize;
        let location = BusinessLocation {
            id: current_id,
            name: Arc::clone(&pools.names[random_number]),
            industry: Arc::clone(&pools.industries[random_number]),
            revenue: rng.gen_range(100000.0..100000000.0),
            employees: rng.gen_range(10..10000),
            city: Arc::clone(&pools.cities[random_number]),
            state: Arc::clone(&pools.states[random_number]),
            country: Arc::clone(&pools.countries[rng.gen_range(0..pools.countries.len())]),
        };

        location.write_to_buffer(&mut buffer, pretty, separator);
        unsafe {
            let progress_locked = Arc::get_mut_unchecked(&mut progress);
            progress_locked.force_unlock();
            progress_locked.get_mut().update(buffer.len());
        }
        // Update progress less frequently
        if count % 50 == 0 {
            progress.lock().print_progress();
        }

        count += 1;
        current_id += 1;
    }

    ChunkResult {
        data: buffer.to_vec(),
        count,
    }
}

// Rest of the implementation remains similar, with progress tracking optimizations
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

        print!("\x1B[6;1H\r\x1B[K");
        print!(
            "Generating data: [{}{}] {:.2}MB/{:.2}MB ({:.2}%)",
            "=".repeat(filled),
            ".".repeat(remaining),
            self.current_size,
            self.target_size,
            percentage * 100.0
        );
        stdout().flush().unwrap();

        if self.current_size >= self.target_size {
            println!("\nGeneration complete!");
        }
    }
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
        (target_size / BYTE_SIZE) as f64,
    )));

    print!("\x1B[2J\x1B[1;1H");
    println!("Starting new data generation request:");
    println!("------------------------------------");
    println!("Requested size: {:.2}MB", target_size / BYTE_SIZE);
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

    println!("Combining Data.");

    // Calculate exact capacity needed
    let total_size: usize = chunks.iter().map(|c| c.data.len()).sum();
    let mut output: Vec<u8> = Vec::with_capacity(total_size + 128); // Extra space for closing bracket

    // Use unsafe extend_from_slice for faster copying
    for chunk in chunks {
        unsafe {
            let old_len = output.len();
            let new_len = old_len + chunk.data.len();
            output.set_len(new_len);
            std::ptr::copy_nonoverlapping(
                chunk.data.as_ptr(),
                output.as_mut_ptr().add(old_len),
                chunk.data.len(),
            );
        }
    }

    output.extend_from_slice(if pretty { b"\n]\n" } else { CLOSE_BRACKET });

    println!("Done.");
    progress.lock().print_progress();

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Encoding", "gzip"))
        .insert_header(("Content-Type", "application/json"))
        .body(output))
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

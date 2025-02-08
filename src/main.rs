#![feature(portable_simd)]
use actix_web::web::Bytes;
use actix_web::{web, App, HttpResponse, HttpServer};
use core::fmt::Error;
use processing::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::HashMap;
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use tokio_stream::wrappers::ReceiverStream;
use util::ProgressInfo;

pub mod processing;
pub mod util;

const BYTE_SIZE: usize = 1024 * 1024;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let num_cpus = num_cpus::get();
    println!("Starting server at http://127.0.0.1:8080");
    println!("Using {} Cores for generation", num_cpus);

    let data_pools = Arc::new(DataPools::new());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(data_pools.clone()))
            .route("/generate", web::get().to(generate_data))
    })
    .bind("127.0.0.1:8080")?
    .workers(num_cpus)
    .run()
    .await
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
) -> Result<HttpResponse, actix_web::Error> {
    let seed: u64 = rand::thread_rng().gen();

    let num_threads = num_cpus::get();
    let target_size = match params.get("size") {
        Some(size) => match parse_size(size) {
            Ok(size) => size,
            Err(_) => BYTE_SIZE,
        },
        None => BYTE_SIZE,
    };
    let chunk_size = target_size / num_threads;

    let pretty_print = params.get("pretty").map_or(false, |v| v == "true");
    let stream_content_type = OutputFormat::from_str(params.get("format").map_or("json", |s| s));

    // Create channels for streaming data
    let (tx, rx) = channel::<Result<Bytes, Error>>(8);
    let (chunk_tx, chunk_rx) = std_mpsc::channel();
    let progress = Arc::new(ProgressInfo::new((target_size / BYTE_SIZE) as f64));

    print!("\x1B[2J\x1B[1;1H");
    println!("Starting new streaming data generation request:");
    println!("------------------------------------");
    println!("Requested size: {:.2}MB", target_size / BYTE_SIZE);
    println!(
        "Format: {}",
        if stream_content_type == OutputFormat::JSON {
            "JSON"
        } else {
            "CSV"
        }
    );

    let data_pools_clone = data_pools.clone();
    let other_prog = progress.clone();

    tokio::spawn(async move {
        if stream_content_type == OutputFormat::CSV {
            let header = b"id,name,industry,revenue,employees,city,state,country\n";
            tx.send(Ok(Bytes::from(header.to_vec()))).await.ok();
        } else {
            tx.send(Ok(Bytes::from(
                if pretty_print { b"[\n" } else { b"[ " }.to_vec(),
            )))
            .await
            .ok();
        }

        std::thread::spawn(move || {
            (0..num_threads).into_par_iter().for_each(|i| {
                let chunk_rng = ChaCha8Rng::seed_from_u64(seed.wrapping_add(i as u64));
                let start_id = (i as u32) * (chunk_size as u32);

                let mut generator = StreamGenerator::new(
                    start_id,
                    chunk_rng,
                    data_pools_clone.clone(),
                    pretty_print,
                    stream_content_type,
                    i == 0,
                    other_prog.clone(),
                    chunk_size,
                );

                while let Some(chunk) = generator.generate_chunk() {
                    other_prog.print_progress();
                    if chunk_tx.send(chunk).is_err() {
                        break;
                    }
                }
            });
        });

        for chunk in chunk_rx {
            if tx.send(Ok(Bytes::from(chunk))).await.is_err() {
                break;
            }
        }

        if stream_content_type == OutputFormat::JSON {
            tx.send(Ok(Bytes::from(
                if pretty_print { b"\n]\n" } else { b"  ]" }.to_vec(),
            )))
            .await
            .ok();
        }
    });

    progress.clone().print_progress();

    let stream = ReceiverStream::new(rx);

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", stream_content_type.content_type()))
        .streaming(stream))
}

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

use util::{convert_error, get_size_info, ProgressInfo};

pub mod processing;
pub mod util;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let num_cpus = num_cpus::get();
    println!("Starting server at http://127.0.0.1:8080");
    println!("Using {} Cores for generation", num_cpus);

    HttpServer::new(move || App::new().route("/generate", web::get().to(generate_data)))
        .bind("127.0.0.1:8080")?
        .workers(num_cpus)
        .run()
        .await
}

async fn generate_data(
    web::Query(params): web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, actix_web::Error> {
    let stream_content_type = OutputFormat::from_str(params.get("format").map_or("json", |s| s));
    let pretty_print = params.get("pretty").map_or(false, |v| v == "true");

    let size_info = get_size_info(params.get("size")).map_err(convert_error)?;
    let (tx, rx) = channel::<Result<Bytes, Error>>(16);

    let progress = Arc::new(ProgressInfo::new(
        size_info.total_size,
        size_info.multiplier,
        size_info.unit,
    ));

    if stream_content_type == OutputFormat::CSV {
        let header = b"id,name,industry,revenue,employees,city,state,country\n";
        progress.update_streamed(header.len());

        tx.send(Ok(Bytes::from(header.to_vec()))).await.ok();
    } else {
        tx.send(Ok(Bytes::from(b"[ ".to_vec()))).await.ok();
    }

    progress.print_header(stream_content_type);
    let sender = tx.clone();
    tokio::spawn(async move {
        let seed: u64 = rand::thread_rng().gen();
        let num_threads = num_cpus::get();
        let chunk_size = size_info.total_size / (num_threads as u64);

        let other_prog = progress.clone();

        let (chunk_tx, chunk_rx) = std_mpsc::sync_channel(16);

        std::thread::spawn(move || {
            let data_pools = DataPools::new();

            (0..num_threads).into_par_iter().for_each(|i| {
                let chunk_rng = ChaCha8Rng::seed_from_u64(seed.wrapping_add(i as u64));
                let start_id = ((i) as u64) * (chunk_size);

                let mut generator = StreamGenerator::new(
                    start_id,
                    chunk_rng,
                    &data_pools,
                    pretty_print,
                    stream_content_type,
                    chunk_size,
                );

                while let Some(chunk) = generator.generate_chunk() {
                    // println!("{}", chunk.len());
                    other_prog.update(chunk.len());
                    other_prog.print_progress();

                    if chunk_tx.send(chunk).is_err() {
                        break;
                    }
                }
            });
        });

        for chunk in chunk_rx {
            progress.update_streamed(chunk.len());
            progress.print_progress();

            if sender.send(Ok(Bytes::from(chunk))).await.is_err() {
                break;
            }
        }

        progress.print_progress();
    });

    if stream_content_type == OutputFormat::JSON {
        tx.send(Ok(Bytes::from(b"  ]".to_vec()))).await.ok();
    }

    let stream = ReceiverStream::new(rx);

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", stream_content_type.content_type()))
        .streaming(stream))
}

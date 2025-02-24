// use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
// use rand::SeedableRng;
// use rand_chacha::ChaCha8Rng;
// use std::time::Duration;
// use sysinfo::{Pid, System};

// use fast_json_gen::processing::{DataPools, OutputFormat, StreamGenerator};

// struct MemorySnapshot {
//     rss: u64,

//     timestamp: std::time::Instant,
// }

// impl MemorySnapshot {
//     fn new() -> Self {
//         let mut sys = System::new_all();
//         sys.refresh_all();

//         let pid = Pid::from(std::process::id() as usize);
//         if let Some(process) = sys.process(pid) {
//             MemorySnapshot {
//                 rss: process.memory() * 1024, // Convert KB to bytes
//                 // Will be updated during measurement
//                 timestamp: std::time::Instant::now(),
//             }
//         } else {
//             MemorySnapshot {
//                 rss: 0,

//                 timestamp: std::time::Instant::now(),
//             }
//         }
//     }

//     fn diff_from(&self, other: &MemorySnapshot) -> (i64, Duration) {
//         let memory_diff = self.rss as i64 - other.rss as i64;
//         let time_diff = self.timestamp.duration_since(other.timestamp);
//         (memory_diff, time_diff)
//     }
// }

// fn format_bytes(bytes: i64) -> String {
//     const KB: f64 = 1024.0;
//     const MB: f64 = KB * 1024.0;
//     const GB: f64 = MB * 1024.0;

//     let bytes = bytes as f64;
//     if bytes.abs() >= GB {
//         format!("{:+.2} MB", bytes / MB) // Convert GB to MB for more reasonable numbers
//     } else if bytes.abs() >= MB {
//         format!("{:+.2} MB", bytes / MB)
//     } else if bytes.abs() >= KB {
//         format!("{:+.2} KB", bytes / KB)
//     } else {
//         format!("{:+.0} B", bytes)
//     }
// }

// fn measure_memory_for_generator(size: u64, format: OutputFormat) -> (u64, i64) {
//     // Take initial measurement
//     let baseline = MemorySnapshot::new();
//     std::thread::sleep(Duration::from_millis(10));

//     let mut peak_usage = 0;

//     // Create generator and measure
//     {
//         let data_pools = DataPools::new();
//         let rng = ChaCha8Rng::seed_from_u64(42);
//         let mut generator = StreamGenerator::new(1, rng, &data_pools, false, format, size);

//         // Track memory during generation
//         while let Some(chunk) = generator.generate_chunk() {
//             black_box(chunk);
//             let current = MemorySnapshot::new();
//             if current.rss > baseline.rss {
//                 peak_usage = peak_usage.max(current.rss - baseline.rss);
//             }
//         }
//     }

//     // Measure final state after cleanup
//     std::thread::sleep(Duration::from_millis(100)); // Give more time for memory to settle
//     let final_snapshot = MemorySnapshot::new();
//     let (memory_diff, _) = final_snapshot.diff_from(&baseline);

//     (peak_usage, memory_diff)
// }

// fn benchmark_data_generation(c: &mut Criterion) {
//     let mut group = c.benchmark_group("data_generation");
//     group.measurement_time(Duration::from_secs(20));
//     group.sample_size(10);

//     // Take initial baseline before any benchmarks
//     let initial_baseline = MemorySnapshot::new();
//     println!(
//         "\nInitial baseline RSS: {}",
//         format_bytes(initial_baseline.rss as i64)
//     );

//     // Test different data sizes
//     let sizes = [
//         ("1MB", 1024 * 1024),
//         ("10MB", 10 * 1024 * 1024),
//         ("100MB", 100 * 1024 * 1024),
//     ];

//     for (size_name, size) in sizes.iter() {
//         // Benchmark and measure JSON
//         group.bench_with_input(
//             BenchmarkId::new("json_throughput", size_name),
//             size,
//             |b, &size| {
//                 b.iter(|| {
//                     let data_pools = DataPools::new();
//                     let rng = ChaCha8Rng::seed_from_u64(42);
//                     let mut generator =
//                         StreamGenerator::new(1, rng, &data_pools, false, OutputFormat::JSON, size);

//                     while let Some(chunk) = generator.generate_chunk() {
//                         black_box(chunk);
//                     }
//                 });
//             },
//         );

//         let (peak_usage, final_diff) = measure_memory_for_generator(*size, OutputFormat::JSON);
//         println!("\nJSON Memory Usage for {}:", size_name,);
//         println!("  Peak Usage: {}", format_bytes(peak_usage as i64));
//         println!("  Retained after cleanup: {}", format_bytes(final_diff));

//         // Benchmark and measure CSV
//         group.bench_with_input(
//             BenchmarkId::new("csv_throughput", size_name),
//             size,
//             |b, &size| {
//                 b.iter(|| {
//                     let data_pools = DataPools::new();
//                     let rng = ChaCha8Rng::seed_from_u64(42);
//                     let mut generator =
//                         StreamGenerator::new(1, rng, &data_pools, false, OutputFormat::CSV, size);

//                     while let Some(chunk) = generator.generate_chunk() {
//                         black_box(chunk);
//                     }
//                 });
//             },
//         );

//         let (peak_usage, final_diff) = measure_memory_for_generator(*size, OutputFormat::CSV);
//         println!("\nCSV Memory Usage for {}:", size_name,);
//         println!("  Peak Usage: {}", format_bytes(peak_usage as i64));
//         println!("  Retained after cleanup: {}", format_bytes(final_diff));
//     }

//     // Take final measurement after all benchmarks
//     let final_snapshot = MemorySnapshot::new();
//     let (total_memory_diff, total_time) = final_snapshot.diff_from(&initial_baseline);

//     println!("\nOverall Memory Summary:");
//     println!(
//         "  Total memory difference: {}",
//         format_bytes(total_memory_diff)
//     );
//     println!("  Total benchmark time: {:.2?}", total_time);

//     group.finish();
// }

// criterion_group!(
//     name = benches;
//     config = Criterion::default()
//         .measurement_time(Duration::from_secs(20))
//         .sample_size(10);
//     targets = benchmark_data_generation
// );
// criterion_main!(benches);

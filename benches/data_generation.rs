use actix_web::web;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

use json_gen_actix::processing::{
    write_location_csv_simd, write_location_json_simd, BusinessLocation, DataPools, JsonPatterns,
    OutputFormat, StreamGenerator,
};

fn bench_formats(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_generation");
    let data_pools = web::Data::new(Arc::new(DataPools::new()));
    let rng = ChaCha8Rng::seed_from_u64(42);

    for size in [64, 256, 1024].iter() {
        group.bench_with_input(BenchmarkId::new("json_pretty", size), size, |b, &size| {
            b.iter(|| {
                let mut generator = StreamGenerator::new(
                    0,
                    rng.clone(),
                    data_pools.clone(),
                    true,
                    OutputFormat::JSON,
                    size * 1024,
                );
                generator.generate_chunk()
            })
        });

        group.bench_with_input(BenchmarkId::new("json_compact", size), size, |b, &size| {
            b.iter(|| {
                let mut generator = StreamGenerator::new(
                    0,
                    rng.clone(),
                    data_pools.clone(),
                    false,
                    OutputFormat::JSON,
                    size * 1024,
                );
                generator.generate_chunk()
            })
        });

        group.bench_with_input(BenchmarkId::new("csv", size), size, |b, &size| {
            b.iter(|| {
                let mut generator = StreamGenerator::new(
                    0,
                    rng.clone(),
                    data_pools.clone(),
                    false,
                    OutputFormat::CSV,
                    size * 1024,
                );
                generator.generate_chunk()
            })
        });
    }
    group.finish();
}

fn bench_simd_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_ops");
    let location = BusinessLocation {
        id: 1,
        name: "Test Company".into(),
        industry: "Technology".into(),
        revenue: 1000000.0,
        employees: 100,
        city: "Test City".into(),
        state: "Test State".into(),
        country: "Test Country".into(),
    };

    group.bench_function("json_simd_write", |b| {
        b.iter(|| {
            let mut output = Vec::with_capacity(1024);
            write_location_json_simd(&location, &mut output, true, &JsonPatterns::new());
        })
    });

    group.bench_function("csv_simd_write", |b| {
        b.iter(|| {
            let mut output = Vec::with_capacity(1024);
            write_location_csv_simd(&location, &mut output);
        })
    });
    group.finish();
}

fn bench_data_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_gen");
    let data_pools = web::Data::new(Arc::new(DataPools::new()));
    let rng = ChaCha8Rng::seed_from_u64(42);

    group.bench_function("location_gen", |b| {
        b.iter(|| BusinessLocation {
            id: 1,
            name: data_pools.names[0].clone(),
            industry: data_pools.industries[0].clone(),
            revenue: rng.clone().gen_range(100000.0..100000000.0),
            employees: rng.clone().gen_range(10..10000),
            city: data_pools.cities[0].clone(),
            state: data_pools.states[0].clone(),
            country: data_pools.countries[0].clone(),
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_formats,
    bench_simd_operations,
    bench_data_generation
);
criterion_main!(benches);

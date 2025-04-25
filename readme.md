# Actix Data Generator

A high-performance random data generation service built with Rust and Actix Web. This tool generates large volumes of synthetic business data on demand with configurable formats and sizes.

## Features

- **High Performance**: Uses SIMD instructions, parallel processing, and optimized memory management for maximum speed
- **Configurable Output Sizes**: Generate data from kilobytes (KB) to terabytes (TB)
- **Multiple Output Formats**: Support for JSON and CSV output formats
- **Pretty Printing**: Optional JSON pretty printing for improved readability
- **Real-time Progress Tracking**: Visual progress indicator during data generation
- **Simulated Business Data**: Generates realistic business records with company names, industries, locations, etc.
- **Web Service Interface**: Simple HTTP API for easy integration with other tools

## Usage

Start the server:

```sh
cargo run --release
```

Then make requests to generate data:

```sh
# Generate 1.5GB of JSON data
curl "http://127.0.0.1:8080/generate?size=1500mb&format=json"

# Generate 100MB of CSV data
curl "http://127.0.0.1:8080/generate?size=100mb&format=csv"

# Generate 50MB of pretty-printed JSON data
curl "http://127.0.0.1:8080/generate?size=50mb&format=json&pretty=true"
```

## API Parameters

- **size**: Specifies the target size of the generated content (required)
  - Supported units: KB, MB, GB, TB
  - Example: `1500mb`, `2gb`, `500kb`

- **format**: Specifies the output format (optional)
  - Supported values: `json` (default), `csv`
  
- **pretty**: Enable pretty-printing for JSON output (optional)
  - Supported values: `true`, `false` (default)

## Data Structure

The generated data contains business records with the following fields:

- **id**: Unique identifier
- **name**: Company name
- **industry**: Business industry/sector
- **revenue**: Annual revenue (floating point number)
- **employees**: Number of employees (integer)
- **city**: City location
- **state**: State/province location
- **country**: Country location

## Implementation Details

- Built with Actix Web for HTTP request handling
- Uses Rayon for parallel data generation
- Optimizes memory usage through efficient chunking
- Implements SIMD (Single Instruction, Multiple Data) operations for faster string processing
- Distributes workload across available CPU cores

## Known Issues

- Progress indicator may not accurately reflect the exact percentage of completion

## Performance Optimization Opportunities

While this generator is performant, there are several opportunities for optimization that contributors could assist. Each section below describes the issue, potential solutions, and implementation approaches being researched.

### 1. Progress Tracking Refinement

**Issue**: The current progress tracking mechanism updates and prints after every chunk generation, causing unnecessary I/O overhead.

**Potential Solutions**:
- Implement time-based or percentage-based thresholds for progress updates
- Use an atomic counter for internal tracking with less frequent display updates
- Add a configuration option to disable progress tracking for maximum performance

**Implementation Approach**:
```rust
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct ThrottledProgress {
    inner: Arc<ProgressInfo>,
    last_update: Mutex<Instant>,
    update_interval: Duration,
}

impl ThrottledProgress {
    pub fn new(inner: Arc<ProgressInfo>, interval_ms: u64) -> Self {
        Self {
            inner,
            last_update: Mutex::new(Instant::now()),
            update_interval: Duration::from_millis(interval_ms),
        }
    }

    pub fn update(&self, bytes: usize) {
        self.inner.update(bytes);
        
        // Only print progress at specified intervals
        let mut last_update = self.last_update.lock().unwrap();
        if last_update.elapsed() >= self.update_interval {
            self.inner.print_progress();
            *last_update = Instant::now();
        }
    }
}
```

### 2. Channel Configuration Optimization

**Issue**: The synchronous channel with zero capacity (`std_mpsc::sync_channel(0)`) forces producers to block until consumers read each message.

**Potential Solutions**:
- Experiment with different channel capacities to find optimal throughput
- Implement a more sophisticated producer-consumer pattern
- Consider using crossbeam channels for potentially better performance

**Implementation Approach**:
```rust
// In main.rs, replace the sync_channel with configurable capacity
let channel_capacity = 4; // Experiment with different values
let (chunk_tx, chunk_rx) = std_mpsc::sync_channel(channel_capacity);

// For more advanced scenarios, consider crossbeam channels:
// use crossbeam_channel as cb;
// let (chunk_tx, chunk_rx) = cb::bounded(channel_capacity);
```

### 3. Memory Management Improvements

**Issue**: Large buffer allocations may cause memory pressure, especially for huge data generation tasks.

**Potential Solutions**:
- Implement a buffer pool to reuse allocated memory
- Fine-tune the `OPTIMAL_CHUNK_SIZE` and `MAX_RECORDS_PER_CHUNK` constants
- Add configurable memory limits to prevent excessive allocations

**Implementation Approach**:
```rust
use bytes::{BytesMut, Bytes};
use std::sync::{Arc, Mutex};

struct BufferPool {
    buffers: Mutex<Vec<BytesMut>>,
    default_capacity: usize,
}

impl BufferPool {
    pub fn new(default_capacity: usize, initial_count: usize) -> Arc<Self> {
        let mut buffers = Vec::with_capacity(initial_count);
        
        // Pre-allocate some buffers
        for _ in 0..initial_count {
            buffers.push(BytesMut::with_capacity(default_capacity));
        }
        
        Arc::new(Self {
            buffers: Mutex::new(buffers),
            default_capacity,
        })
    }
    
    pub fn get_buffer(&self) -> BytesMut {
        let mut pool = self.buffers.lock().unwrap();
        pool.pop().unwrap_or_else(|| BytesMut::with_capacity(self.default_capacity))
    }
    
    pub fn return_buffer(&self, mut buffer: BytesMut) {
        buffer.clear(); // Reset position but keep capacity
        let mut pool = self.buffers.lock().unwrap();
        pool.push(buffer);
    }
}
```

### 4. Adaptive Chunking Strategy

**Issue**: Fixed chunk sizes may not be optimal for all data patterns and hardware configurations.

**Potential Solutions**:
- Implement adaptive chunk sizing based on system resources and request size
- Add runtime configuration options for chunk size parameters
- Create a feedback mechanism that adjusts chunk size based on processing speed

**Implementation Approach**:
```rust
// In StreamGenerator, add fields to track performance
pub struct StreamGenerator<'a> {
    // ...existing fields...
    last_chunk_duration: Option<Duration>,
    target_chunk_duration: Duration,
}

impl<'a> StreamGenerator<'a> {
    // In generate_chunk method
    pub fn generate_chunk(&mut self) -> Option<Bytes> {
        let start_time = Instant::now();
        
        // Adjust chunk_target based on previous performance
        let mut chunk_target = self.chunk_size.min(OPTIMAL_CHUNK_SIZE);
        
        if let Some(last_duration) = self.last_chunk_duration {
            // If previous chunk was too slow, reduce size
            if last_duration > self.target_chunk_duration * 1.2 {
                chunk_target = (chunk_target as f64 * 0.8) as u64;
            } 
            // If previous chunk was fast, increase size
            else if last_duration < self.target_chunk_duration * 0.8 {
                chunk_target = (chunk_target as f64 * 1.2) as u64;
            }
        }
        
        // ... existing chunk generation logic ...
        
        // Record duration for next adjustment
        self.last_chunk_duration = Some(start_time.elapsed());
        
        // Return the generated chunk
        if !buffer.is_empty() {
            Some(buffer.into())
        } else {
            None
        }
    }
}
```

### 5. SIMD Optimization

**Issue**: SIMD operations may not be optimized for all hardware platforms.

**Potential Solutions**:
- Add conditional compilation for different CPU architectures
- Create fallback paths for platforms where SIMD operations might be slower
- Benchmark different SIMD implementations to find the most efficient approach

**Implementation Approach**:
```rust
// Using conditional compilation for SIMD optimization
#[cfg(target_feature = "avx2")]
pub fn process_string_simd(input: &[u8]) -> Vec<u8> {
    // Use AVX2 instructions for 256-bit SIMD
    // ...implementation...
}

#[cfg(all(target_feature = "sse2", not(target_feature = "avx2")))]
pub fn process_string_simd(input: &[u8]) -> Vec<u8> {
    // Use SSE2 instructions for 128-bit SIMD
    // ...implementation...
}

#[cfg(not(any(target_feature = "sse2", target_feature = "avx2")))]
pub fn process_string_simd(input: &[u8]) -> Vec<u8> {
    // Fallback for platforms without SIMD support
    // ...implementation...
}
```

### 6. Backpressure Handling

**Issue**: The current implementation might not provide adequate backpressure for very large data generations.

**Potential Solutions**:
- Implement a more sophisticated flow control mechanism
- Add configurable rate limiting
- Create an adaptive system that responds to consumer consumption rates

**Implementation Approach**:
```rust
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{channel, Sender};

pub struct RateLimitedChannel<T> {
    tx: Sender<T>,
    rate_limit: usize,  // items per second
    window_start: Instant,
    items_in_window: usize,
}

impl<T> RateLimitedChannel<T> {
    pub fn new(tx: Sender<T>, rate_limit: usize) -> Self {
        Self {
            tx,
            rate_limit,
            window_start: Instant::now(),
            items_in_window: 0,
        }
    }
    
    pub async fn send(&mut self, item: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        // Check if we need to start a new window
        let elapsed = self.window_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            // Reset window
            self.window_start = Instant::now();
            self.items_in_window = 0;
        }
        
        // Check if we've exceeded our rate limit
        if self.items_in_window >= self.rate_limit {
            let sleep_time = Duration::from_secs(1).checked_sub(elapsed).unwrap_or_default();
            tokio::time::sleep(sleep_time).await;
            self.window_start = Instant::now();
            self.items_in_window = 0;
        }
        
        // Send item and update counter
        self.items_in_window += 1;
        self.tx.send(item).await
    }
}
```

### 7. Thread Pool Configuration

**Issue**: Using `num_cpus::get()` for thread count might not be optimal for all workloads.

**Potential Solutions**:
- Add configuration options for thread pool size
- Implement workload-based thread scaling
- Create a more sophisticated work-stealing algorithm for better CPU utilization

**Implementation Approach**:
```rust
// In main.rs
async fn main() -> std::io::Result<()> {
    // Get optimal worker count from environment or default to number of CPUs
    let workers = std::env::var("ACTIX_WORKERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| num_cpus::get());
        
    println!("Starting server at http://127.0.0.1:8080");
    println!("Using {} worker threads", workers);

    HttpServer::new(move || App::new().route("/generate", web::get().to(generate_data)))
        .bind("127.0.0.1:8080")?
        .workers(workers)
        .run()
        .await
}
```

### 8. Cache Optimization

**Issue**: Current cache alignment strategies may not be optimal across different CPU architectures.

**Potential Solutions**:
- Profile and optimize memory access patterns
- Improve data structure alignment
- Implement more efficient padding strategies

**Implementation Approach**:
```rust
use std::alloc::{Layout, alloc, dealloc};

// Cache-aligned vector implementation
pub struct AlignedVec<T> {
    ptr: *mut T,
    len: usize,
    capacity: usize,
}

impl<T> AlignedVec<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        let size = std::mem::size_of::<T>() * capacity;
        let align = 64; // Cache line size
        
        unsafe {
            let layout = Layout::from_size_align_unchecked(size, align);
            let ptr = alloc(layout) as *mut T;
            
            Self {
                ptr,
                len: 0,
                capacity,
            }
        }
    }
    
    // Implement other vector methods...
}

impl<T> Drop for AlignedVec<T> {
    fn drop(&mut self) {
        if self.capacity > 0 {
            unsafe {
                let size = std::mem::size_of::<T>() * self.capacity;
                let align = 64; // Cache line size
                let layout = Layout::from_size_align_unchecked(size, align);
                dealloc(self.ptr as *mut u8, layout);
            }
        }
    }
}
```

## Contributing

If you'd like to implement one of these optimizations or have other improvements in mind, please:

1. Fork the repository
2. Create a feature branch (`git checkout -b my-optimization`)
3. Implement your changes with appropriate tests
4. Benchmark your changes to verify performance improvements
5. Submit a pull request

For larger changes, consider opening an issue first to discuss your approach.


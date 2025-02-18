use bytes::{BufMut, Bytes, BytesMut};
#[cfg(target_arch = "x86_64")]
use fake::{
    faker::{address::en::*, company::en::*},
    Fake,
};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Serialize;
use std::simd::u8x32;

const BYTE_COUNT: usize = 32;
const POOL_SIZE: i32 = 1000;
const OPTIMAL_CHUNK_SIZE: u64 = 16 * 1024;
const MAX_RECORDS_PER_CHUNK: u64 = 1000;

// Reference-based version of BusinessLocation to avoid allocations
pub struct BusinessLocationRef<'a> {
    id: u64,
    name: &'a str,
    industry: &'a str,
    revenue: f32,
    employees: u32,
    city: &'a str,
    state: &'a str,
    country: &'a str,
}
pub struct StreamGenerator<'a> {
    current_id: u64,
    rng: ChaCha8Rng,
    pools: &'a DataPools,
    pretty: bool,
    format: OutputFormat,
    json_patterns: JsonPatterns,
    bytes_generated: u64,
    chunk_size: u64,
}

impl<'a> StreamGenerator<'a> {
    pub fn new(
        start_id: u64,
        rng: ChaCha8Rng,
        pools: &'a DataPools,
        pretty: bool,
        format: OutputFormat,
        chunk_size: u64,
    ) -> Self {
        Self {
            current_id: start_id,
            rng,
            pools,
            pretty,
            format,
            json_patterns: JsonPatterns::new(),
            bytes_generated: 0,
            chunk_size,
        }
    }
    #[inline]
    pub fn generate_chunk(&mut self) -> Option<Bytes> {
        if self.bytes_generated >= self.chunk_size {
            return None;
        }

        let chunk_target = (OPTIMAL_CHUNK_SIZE).min(self.chunk_size - self.bytes_generated);
        let max_records = (chunk_target / 100).min(MAX_RECORDS_PER_CHUNK);

        let start_id = self.current_id;
        let locations: Vec<_> = (0..max_records)
            .into_par_iter()
            .map(|offset| {
                let mut local_rng = self.rng.clone();
                local_rng.set_stream(offset);

                let random_number = local_rng.gen_range(0..100);

                BusinessLocationRef {
                    id: start_id + offset,
                    name: &self.pools.names[random_number],
                    industry: &self.pools.industries[random_number],
                    revenue: local_rng.gen_range(100000.0..100000000.0),
                    employees: local_rng.gen_range(10..10000),
                    city: &self.pools.cities[random_number],
                    state: &self.pools.states[random_number],
                    country: &self.pools.countries[local_rng.gen_range(0..5)],
                }
            })
            .collect();

        let mut buffer = BytesMut::with_capacity(OPTIMAL_CHUNK_SIZE as usize);
        for location in locations {
            let start_len = buffer.len();

            match self.format {
                OutputFormat::JSON => {
                    self.write_location_json_simd(&location, &mut buffer);
                }
                OutputFormat::CSV => {
                    self.write_location_csv_simd(&location, &mut buffer);
                }
            }

            let bytes_written = buffer.len() - start_len;
            self.bytes_generated += bytes_written as u64;

            if self.bytes_generated >= self.chunk_size {
                break;
            }
        }

        self.current_id += 1;

        if !buffer.is_empty() {
            Some(buffer.into())
        } else {
            None
        }
    }
    // #[inline]
    // pub fn generate_chunk(&mut self) -> Option<Bytes> {
    //     if self.bytes_generated >= self.chunk_size {
    //         return None;
    //     }

    //     let chunk_target = (OPTIMAL_CHUNK_SIZE).min(self.chunk_size - self.bytes_generated);
    //     let max_records = (chunk_target / 100).min(MAX_RECORDS_PER_CHUNK);
    //     let start_id = self.current_id;

    //     // Pre-allocate a fixed buffer for the chunk
    //     let mut buffer = BytesMut::with_capacity(OPTIMAL_CHUNK_SIZE as usize);
    //     let mut bytes_written = 0;
    //     let mut records_processed = 0;

    //     // Process records one at a time, writing directly to the buffer
    //     for offset in 0..max_records {
    //         let mut local_rng = self.rng.clone();
    //         local_rng.set_stream(offset);
    //         let random_number = local_rng.gen_range(0..100);

    //         // Create location data without allocating strings
    //         let location = BusinessLocationRef {
    //             id: start_id + offset,
    //             name: &self.pools.names[random_number],
    //             industry: &self.pools.industries[random_number],
    //             revenue: local_rng.gen_range(100000.0..100000000.0),
    //             employees: local_rng.gen_range(10..10000),
    //             city: &self.pools.cities[random_number],
    //             state: &self.pools.states[random_number],
    //             country: &self.pools.countries[local_rng.gen_range(0..5)],
    //         };

    //         let start_pos = buffer.len();

    //         // Write directly to the buffer
    //         match self.format {
    //             OutputFormat::JSON => {
    //                 self.write_location_json_simd(&location, &mut buffer);
    //             }
    //             OutputFormat::CSV => {
    //                 self.write_location_csv_simd(&location, &mut buffer);
    //             }
    //         }

    //         bytes_written += buffer.len() - start_pos;
    //         self.bytes_generated += bytes_written as u64;
    //         records_processed += 1;

    //         if self.bytes_generated >= self.chunk_size {
    //             break;
    //         }
    //     }

    //     self.current_id += records_processed;

    //     if bytes_written > 0 {
    //         Some(buffer.freeze())
    //     } else {
    //         None
    //     }
    // }

    pub fn estimate_objects_per_chunk(&self) -> u64 {
        let avg_object_size = match self.format {
            OutputFormat::JSON => {
                if self.pretty {
                    200
                } else {
                    150
                }
            }
            OutputFormat::CSV => 100,
        };

        self.chunk_size / avg_object_size
    }

    #[inline(always)]
    pub fn write_location_json_simd(
        &mut self,
        location: &BusinessLocationRef,
        buffer: &mut BytesMut,
    ) {
        buffer.put_u8(b',');

        buffer.put_u8(b'{');

        // Write numeric fields
        let mut id_buf = itoa::Buffer::new();
        let mut emp_buf = itoa::Buffer::new();
        let mut rev_buf = dtoa::Buffer::new();

        let id_str = id_buf.format(location.id);
        let revenue_str = rev_buf.format(location.revenue);
        let employees_str = emp_buf.format(location.employees);

        let (separator, ending) = (
            self.json_patterns.separator_compact,
            self.json_patterns.ending_compact,
        );

        let numeric_values = [id_str, revenue_str, employees_str];
        for (i, value) in numeric_values.iter().enumerate() {
            if i > 0 {
                buffer.extend_from_slice(&separator[..]);
            }
            buffer.extend_from_slice(
                &self.json_patterns.unquoted_field_patterns[i].prefix
                    [..self.json_patterns.unquoted_field_patterns[i].prefix_len],
            );
            buffer.extend_from_slice(value.as_bytes());
        }

        // Write string fields
        let string_values = [
            location.name,
            location.industry,
            location.city,
            location.state,
            location.country,
        ];

        for (pattern, value) in self
            .json_patterns
            .quoted_field_patterns
            .iter()
            .zip(string_values.iter())
        {
            buffer.extend_from_slice(&separator[..]);
            buffer.extend_from_slice(&pattern.prefix[..]);

            // Use SIMD for string copy
            let bytes = value.as_bytes();
            let chunks = bytes.chunks(BYTE_COUNT);
            for chunk in chunks {
                if chunk.len() == BYTE_COUNT {
                    let simd_chunk = u8x32::from_slice(chunk);
                    buffer.extend_from_slice(&simd_chunk.to_array());
                } else {
                    buffer.extend_from_slice(chunk);
                }
            }

            buffer.extend_from_slice(&pattern.suffix[..]);
        }

        buffer.extend_from_slice(&ending[..]);
    }

    #[inline]
    pub fn write_location_csv_simd(
        &mut self,
        location: &BusinessLocationRef,
        buffer: &mut BytesMut,
    ) {
        // Write id
        let mut id_buf = itoa::Buffer::new();
        let id_str = id_buf.format(location.id);
        buffer.extend_from_slice(id_str.as_bytes());
        buffer.put_u8(b',');

        // Write string fields with SIMD
        let string_fields = [
            location.name,
            location.industry,
            &location.revenue.to_string(),
            &location.employees.to_string(),
            location.city,
            location.state,
            location.country,
        ];

        for (i, field) in string_fields.iter().enumerate() {
            let bytes = field.as_bytes();
            let chunks = bytes.chunks(BYTE_COUNT);

            for chunk in chunks {
                if chunk.len() == BYTE_COUNT {
                    let simd_chunk = u8x32::from_slice(chunk);
                    buffer.extend_from_slice(&simd_chunk.to_array());
                } else {
                    buffer.extend_from_slice(chunk);
                }
            }

            if i < string_fields.len() - 1 {
                buffer.put_u8(b',');
            }
        }

        buffer.put_u8(b'\n');
    }
}

pub struct JsonPatterns {
    separator_compact: [u8; 32],
    ending_compact: [u8; 32],
    quoted_field_patterns: [QuotedFieldPattern; 5],
    unquoted_field_patterns: [UnquotedFieldPattern; 3],
}

impl JsonPatterns {
    pub fn new() -> Self {
        let mut aligned = AlignedPatterns {
            numeric_prefixes: [[0; 64]; 3],
            string_prefixes: [[0; 64]; 5],
            string_suffixes: [[0; 64]; 5],
        };

        for (i, field) in ["id", "revenue", "employees"].iter().enumerate() {
            aligned.numeric_prefixes[i][0] = b'"';
            aligned.numeric_prefixes[i][1..1 + field.len()].copy_from_slice(field.as_bytes());
            aligned.numeric_prefixes[i][1 + field.len()..1 + field.len() + 3]
                .copy_from_slice(b"\": ");
        }

        for (i, field) in ["name", "industry", "city", "state", "country"]
            .iter()
            .enumerate()
        {
            aligned.string_prefixes[i][0] = b'"';
            aligned.string_prefixes[i][1..1 + field.len()].copy_from_slice(field.as_bytes());
            aligned.string_prefixes[i][1 + field.len()..1 + field.len() + 4]
                .copy_from_slice(b"\": \"");
            aligned.string_suffixes[i][0] = b'"';
        }

        // Initialize separators
        let mut separator_pretty = [0u8; 32];
        separator_pretty[..6].copy_from_slice(b",\n    ");
        let mut separator_compact = [0u8; 32];
        separator_compact[0] = b',';

        let mut ending_pretty = [0u8; 32];
        ending_pretty[..4].copy_from_slice(b"\n  }");
        let mut ending_compact = [0u8; 32];
        ending_compact[0] = b'}';

        Self {
            separator_compact,
            ending_compact,
            quoted_field_patterns: aligned
                .string_prefixes
                .iter()
                .zip(aligned.string_suffixes.iter())
                .map(|(prefix, suffix)| QuotedFieldPattern {
                    prefix: prefix[..32].try_into().unwrap(),
                    suffix: suffix[..32].try_into().unwrap(),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            unquoted_field_patterns: aligned
                .numeric_prefixes
                .iter()
                .map(|prefix| UnquotedFieldPattern {
                    prefix: prefix[..32].try_into().unwrap(),
                    prefix_len: 32,
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct BusinessLocation {
    pub id: u64,
    pub name: String,
    pub industry: String,
    pub revenue: f32,
    pub employees: u32,
    pub city: String,
    pub state: String,
    pub country: String,
}

#[derive(PartialEq, Clone, Copy)]
pub enum OutputFormat {
    JSON,
    CSV,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "csv" => OutputFormat::CSV,
            _ => OutputFormat::JSON,
        }
    }
    pub fn to_string(&self) -> &str {
        match self {
            OutputFormat::JSON => "JSON",
            OutputFormat::CSV => "CSV",
        }
    }

    pub fn content_type(&self) -> &str {
        match self {
            OutputFormat::JSON => "application/json",
            OutputFormat::CSV => "text/csv",
        }
    }
}

pub struct DataPools {
    pub names: Vec<String>,
    pub cities: Vec<String>,
    pub states: Vec<String>,
    pub countries: Vec<String>,
    pub industries: Vec<String>,
}

impl DataPools {
    pub fn new() -> Self {
        DataPools {
            names: (0..POOL_SIZE).map(|_| CompanyName().fake()).collect(),
            industries: (0..POOL_SIZE).map(|_| Industry().fake()).collect(),
            cities: (0..POOL_SIZE).map(|_| CityName().fake()).collect(),
            states: (0..POOL_SIZE).map(|_| StateName().fake()).collect(),
            countries: (0..50).map(|_| CountryName().fake()).collect(),
        }
    }
}

#[derive(Debug)]
struct QuotedFieldPattern {
    prefix: [u8; 32],
    suffix: [u8; 32],
}
#[derive(Debug)]
struct UnquotedFieldPattern {
    prefix: [u8; 32],
    prefix_len: usize,
}
#[repr(align(64))]
struct AlignedPatterns {
    numeric_prefixes: [[u8; 64]; 3],
    string_prefixes: [[u8; 64]; 5],
    string_suffixes: [[u8; 64]; 5],
}

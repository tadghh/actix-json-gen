#[cfg(target_arch = "x86_64")]
use fake::{
    faker::{address::en::*, company::en::*},
    Fake,
};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Serialize;
use std::{arch::x86_64::*, simd::u8x32};

const BYTE_COUNT: usize = 32;
const POOL_SIZE: i32 = 1000;
const OPTIMAL_CHUNK_SIZE: u64 = 16 * 1024;
const MAX_RECORDS_PER_CHUNK: u64 = 1000;
const BUFFER_CAPACITY: usize = 16 * 1024;

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
    pub fn generate_chunk(&mut self) -> Option<Vec<u8>> {
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

                BusinessLocation {
                    id: start_id + offset,
                    name: self.pools.names[random_number].to_owned(),
                    industry: self.pools.industries[random_number]
                        .to_owned()
                        .replace(",", ""),
                    revenue: local_rng.gen_range(100000.0..100000000.0),
                    employees: local_rng.gen_range(10..10000),
                    city: self.pools.cities[random_number].to_owned(),
                    state: self.pools.states[random_number].to_owned(),
                    country: self.pools.countries[local_rng.gen_range(0..5)].to_owned(),
                }
            })
            .collect();

        let estimated_size = match self.format {
            OutputFormat::JSON => locations.len() * 200,
            OutputFormat::CSV => locations.len() * 100,
        };

        let mut output = Vec::with_capacity(estimated_size);

        for location in &locations {
            let start_len = output.len();

            match self.format {
                OutputFormat::JSON => {
                    self.write_location_json_simd(&location, &mut output);
                }
                OutputFormat::CSV => {
                    self.write_location_csv_simd(&location, &mut output);
                }
            }

            let bytes_written = output.len() - start_len;
            self.bytes_generated += bytes_written as u64;

            if self.bytes_generated >= self.chunk_size {
                break;
            }
        }

        self.current_id += locations.len() as u64;

        if !output.is_empty() {
            Some(output)
        } else {
            None
        }
    }

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
    pub fn write_location_json_simd(&mut self, location: &BusinessLocation, output: &mut Vec<u8>) {
        output.push(b',');

        let needed_capacity = location.name.len()
            + location.industry.len()
            + location.city.len()
            + location.state.len()
            + location.country.len()
            + 128;

        output.reserve(needed_capacity);

        let mut id_buf = itoa::Buffer::new();
        let mut emp_buf = itoa::Buffer::new();
        let mut rev_buf = dtoa::Buffer::new();
        let mut buffer = AlignedBuffer {
            data: [0; BUFFER_CAPACITY],
        };

        let id_str = id_buf.format(location.id);
        let revenue_str = rev_buf.format(location.revenue);
        let employees_str = emp_buf.format(location.employees);

        let (separator, ending) = (
            self.json_patterns.separator_compact,
            self.json_patterns.ending_compact,
        );

        unsafe {
            output.push(b'{');

            let mut write_field_simd = |data: &[u8], dst: &mut Vec<u8>| {
                for chunk in data.chunks(64) {
                    _mm_prefetch(chunk.as_ptr() as *const i8, _MM_HINT_T0);
                    buffer.data[..chunk.len()].copy_from_slice(chunk);
                    dst.extend_from_slice(&buffer.data[..chunk.len()]);
                }
            };

            let numeric_values = [id_str, revenue_str, employees_str];

            for (i, value) in numeric_values.iter().enumerate() {
                if i > 0 {
                    output.extend_from_slice(&separator[..]);
                }
                write_field_simd(
                    &self.json_patterns.unquoted_field_patterns[i].prefix
                        [..self.json_patterns.unquoted_field_patterns[i].prefix_len],
                    output,
                );
                write_field_simd(value.as_bytes(), output);
            }

            let string_values = [
                &location.name,
                &location.industry,
                &location.city,
                &location.state,
                &location.country,
            ];

            for (pattern, value) in self
                .json_patterns
                .quoted_field_patterns
                .iter()
                .zip(string_values.iter())
            {
                output.extend_from_slice(&separator[..]);
                write_field_simd(&pattern.prefix[..], output);
                write_field_simd(value.as_bytes(), output);
                write_field_simd(&pattern.suffix[..], output);
            }

            output.extend_from_slice(&ending[..]);
        }
    }

    #[inline]
    pub fn write_location_csv_simd(&mut self, location: &BusinessLocation, output: &mut Vec<u8>) {
        let id_str = location.id.to_string();
        let revenue_str = location.revenue.to_string();
        let employees_str = location.employees.to_string();

        let total_size = id_str.len()
            + location.name.len()
            + location.industry.len()
            + revenue_str.len()
            + employees_str.len()
            + location.city.len()
            + location.state.len()
            + location.country.len()
            + 8;

        output.reserve(total_size);

        self.copy_str_simd(output, &id_str);
        output.push(b',');
        self.copy_str_simd(output, &location.name);
        output.push(b',');
        self.copy_str_simd(output, &location.industry);
        output.push(b',');
        self.copy_str_simd(output, &revenue_str);
        output.push(b',');
        self.copy_str_simd(output, &employees_str);
        output.push(b',');
        self.copy_str_simd(output, &location.city);
        output.push(b',');
        self.copy_str_simd(output, &location.state);
        output.push(b',');
        self.copy_str_simd(output, &location.country);
        output.push(b'\n');
    }

    #[inline]
    fn copy_str_simd(&mut self, output: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        let len = bytes.len();
        if len <= BYTE_COUNT {
            output.extend_from_slice(bytes);
            return;
        }

        let chunks = len / BYTE_COUNT;
        output.reserve(len);

        for chunk in 0..chunks {
            let offset = chunk * BYTE_COUNT;
            let simd_chunk = u8x32::from_slice(&bytes[offset..offset + BYTE_COUNT]);
            output.extend_from_slice(&simd_chunk.to_array());
        }

        let remaining_start = chunks * BYTE_COUNT;
        if remaining_start < len {
            output.extend_from_slice(&bytes[remaining_start..]);
        }
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
#[repr(align(32))]
struct AlignedBuffer {
    data: [u8; BUFFER_CAPACITY],
}
#[repr(align(64))]
struct AlignedPatterns {
    numeric_prefixes: [[u8; 64]; 3],
    string_prefixes: [[u8; 64]; 5],
    string_suffixes: [[u8; 64]; 5],
}

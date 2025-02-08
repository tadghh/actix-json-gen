use crate::util::ProgressInfo;
#[cfg(target_arch = "x86_64")]
use actix_web::web;
use fake::{
    faker::{address::en::*, company::en::*},
    Fake,
};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;
use std::{arch::x86_64::*, simd::u8x32, sync::Arc};

const BYTE_COUNT: usize = 32;
const POOL_SIZE: i32 = 1000;

pub struct JsonPatterns {
    separator_pretty: [u8; 32],
    separator_compact: [u8; 32],
    ending_pretty: [u8; 32],
    ending_compact: [u8; 32],
    quoted_field_patterns: [QuotedFieldPattern; 5],
    unquoted_field_patterns: [UnquotedFieldPattern; 3],
}
#[derive(Debug)]
struct QuotedFieldPattern {
    prefix: [u8; 32],
    suffix: [u8; 32],
    prefix_len: usize,
    suffix_len: usize,
}
#[derive(Debug)]
struct UnquotedFieldPattern {
    prefix: [u8; 32],
    prefix_len: usize,
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
            separator_pretty,
            separator_compact,
            ending_pretty,
            ending_compact,
            quoted_field_patterns: aligned
                .string_prefixes
                .iter()
                .zip(aligned.string_suffixes.iter())
                .map(|(prefix, suffix)| QuotedFieldPattern {
                    prefix: prefix[..32].try_into().unwrap(),
                    suffix: suffix[..32].try_into().unwrap(),
                    prefix_len: 32,
                    suffix_len: 1,
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

#[repr(align(64))]
struct AlignedBuffer {
    data: [u8; 256],
}

#[repr(align(64))]
struct AlignedPatterns {
    numeric_prefixes: [[u8; 64]; 3],
    string_prefixes: [[u8; 64]; 5],
    string_suffixes: [[u8; 64]; 5],
}
#[derive(Serialize)]
pub struct BusinessLocation {
    pub id: u32,
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

#[inline]
pub fn write_location_csv_simd(location: &BusinessLocation, output: &mut Vec<u8>) {
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

    copy_str_simd(output, &id_str);
    output.push(b',');
    copy_str_simd(output, &location.name);
    output.push(b',');
    copy_str_simd(output, &location.industry);
    output.push(b',');
    copy_str_simd(output, &revenue_str);
    output.push(b',');
    copy_str_simd(output, &employees_str);
    output.push(b',');
    copy_str_simd(output, &location.city);
    output.push(b',');
    copy_str_simd(output, &location.state);
    output.push(b',');
    copy_str_simd(output, &location.country);
    output.push(b'\n');
}

#[inline]
fn copy_str_simd(output: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let chunks = len / BYTE_COUNT;

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

#[inline(always)]
pub fn write_location_json_simd(
    location: &BusinessLocation,
    output: &mut Vec<u8>,
    pretty: bool,
    patterns: &JsonPatterns,
    is_first: bool,
) {
    if !is_first {
        output.push(b',');
        if pretty {
            output.extend_from_slice(b",\n  ");
        }
    }

    let mut id_buf = itoa::Buffer::new();
    let mut emp_buf = itoa::Buffer::new();
    let mut rev_buf = dtoa::Buffer::new();
    let mut buffer = AlignedBuffer { data: [0; 256] };

    let id_str = id_buf.format(location.id);
    let revenue_str = rev_buf.format(location.revenue);
    let employees_str = emp_buf.format(location.employees);

    let (separator, ending) = if pretty {
        (&patterns.separator_pretty, &patterns.ending_pretty)
    } else {
        (&patterns.separator_compact, &patterns.ending_compact)
    };

    let needed_capacity = id_str.len()
        + revenue_str.len()
        + employees_str.len()
        + location.name.len()
        + location.industry.len()
        + location.city.len()
        + location.state.len()
        + location.country.len()
        + 256;

    output.reserve(needed_capacity);

    unsafe {
        output.push(b'{');
        if pretty {
            output.extend_from_slice(b"\n    ");
        }

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
                output.extend_from_slice(&separator[..if pretty { 6 } else { 1 }]);
            }
            write_field_simd(
                &patterns.unquoted_field_patterns[i].prefix
                    [..patterns.unquoted_field_patterns[i].prefix_len],
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

        for (pattern, value) in patterns
            .quoted_field_patterns
            .iter()
            .zip(string_values.iter())
        {
            output.extend_from_slice(&separator[..if pretty { 6 } else { 1 }]);
            write_field_simd(&pattern.prefix[..pattern.prefix_len], output);
            write_field_simd(value.as_bytes(), output);
            write_field_simd(&pattern.suffix[..pattern.suffix_len], output);
        }

        output.extend_from_slice(&ending[..if pretty { 4 } else { 1 }]);
    }
}

pub struct StreamGenerator {
    current_id: u32,
    rng: ChaCha8Rng,
    pools: web::Data<Arc<DataPools>>,
    pretty: bool,
    format: OutputFormat,
    is_first: bool,
    progress: Arc<ProgressInfo>,
    json_patterns: JsonPatterns,
    bytes_generated: usize,
    target_size: usize,
}

impl StreamGenerator {
    pub fn new(
        start_id: u32,
        rng: ChaCha8Rng,
        pools: web::Data<Arc<DataPools>>,
        pretty: bool,
        format: OutputFormat,
        is_first: bool,
        progress: Arc<ProgressInfo>,
        target_size: usize,
    ) -> Self {
        Self {
            current_id: start_id,
            rng,
            pools,
            pretty,
            format,
            is_first,
            progress,
            json_patterns: JsonPatterns::new(),
            bytes_generated: 0,
            target_size,
        }
    }

    #[inline]
    pub fn generate_chunk(&mut self) -> Option<Vec<u8>> {
        if self.bytes_generated >= self.target_size {
            return None;
        }

        let mut output = Vec::with_capacity(262144);
        let chunk_target = 262144.min(self.target_size - self.bytes_generated);
        let mut records_in_chunk = 0;
        let mut current_write = 0;

        if records_in_chunk > 0 {
            self.is_first = false;
        }
        while output.len() < chunk_target && records_in_chunk < 1000 {
            let random_number = self.rng.gen_range(0..100);
            let location = BusinessLocation {
                id: self.current_id,
                name: self.pools.names[random_number].as_str().into(),
                industry: self.pools.industries[random_number]
                    .to_owned()
                    .replace(",", ""),
                revenue: self.rng.gen_range(100000.0..100000000.0),
                employees: self.rng.gen_range(10..10000),
                city: self.pools.cities[random_number].as_str().into(),
                state: self.pools.states[random_number].as_str().into(),
                country: self.pools.countries[self.rng.gen_range(0..5)]
                    .as_str()
                    .into(),
            };

            let start_len = output.len();

            match self.format {
                OutputFormat::JSON => {
                    write_location_json_simd(
                        &location,
                        &mut output,
                        self.pretty,
                        &self.json_patterns,
                        self.is_first,
                    );
                }
                OutputFormat::CSV => {
                    write_location_csv_simd(&location, &mut output);
                }
            }

            self.current_id += 1;
            records_in_chunk += 1;

            let bytes_written = output.len() - start_len;
            self.bytes_generated += bytes_written;
            current_write += bytes_written;

            if bytes_written % 60 == 0 {
                self.progress.update(current_write);
                self.progress.print_progress();
                current_write = 0;
            }
        }

        self.progress.update(current_write);

        if !output.is_empty() {
            Some(output)
        } else {
            None
        }
    }
}

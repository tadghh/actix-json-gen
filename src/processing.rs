use crate::ProgressInfo;
use fake::{
    faker::{address::en::*, company::en::*},
    Fake,
};
use parking_lot::Mutex;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;
use std::{simd::u8x32, sync::Arc};

const BYTE_COUNT: usize = 32;
const REFRESH_COUNT: u32 = 1500;
const POOL_SIZE: i32 = 1000;

// Pre-computed patterns for both pretty and compact modes
struct JsonPatterns {
    separator_pretty: [u8; 32],  // ",\n    "
    separator_compact: [u8; 32], // ","
    ending_pretty: [u8; 32],     // "\n  }"
    ending_compact: [u8; 32],    // "}"
    quoted_field_patterns: [QuotedFieldPattern; 5],
    unquoted_field_patterns: [UnquotedFieldPattern; 3],
}

struct QuotedFieldPattern {
    prefix: [u8; 32], // "\"field\": \""
    suffix: [u8; 32], // "\""
    prefix_len: usize,
    suffix_len: usize,
}

// Pre-computed pattern for each unquoted field
struct UnquotedFieldPattern {
    prefix: [u8; 32],
    prefix_len: usize,
}

impl JsonPatterns {
    fn new() -> Self {
        let mut field_start_pretty = [0u8; 32];
        field_start_pretty[..6].copy_from_slice(b"\n    \"");

        let mut field_start_compact = [0u8; 32];
        field_start_compact[0] = b'"';

        let mut separator_pretty = [0u8; 32];
        separator_pretty[..6].copy_from_slice(b",\n    ");

        let mut separator_compact = [0u8; 32];
        separator_compact[..1].copy_from_slice(b",");

        let mut ending_pretty = [0u8; 32];
        ending_pretty[..4].copy_from_slice(b"\n  }");

        let mut ending_compact = [0u8; 32];
        ending_compact[0] = b'}';

        let quoted_fields = [
            ("name", create_quoted_pattern(b"name")),
            ("industry", create_quoted_pattern(b"industry")),
            ("city", create_quoted_pattern(b"city")),
            ("state", create_quoted_pattern(b"state")),
            ("country", create_quoted_pattern(b"country")),
        ]
        .map(|(_, pattern)| pattern);

        let unquoted_fields = [
            ("id", create_unquoted_pattern(b"id")),
            ("revenue", create_unquoted_pattern(b"revenue")),
            ("employees", create_unquoted_pattern(b"employees")),
        ]
        .map(|(_, pattern)| pattern);

        Self {
            separator_pretty,
            separator_compact,
            ending_pretty,
            ending_compact,
            quoted_field_patterns: quoted_fields,
            unquoted_field_patterns: unquoted_fields,
        }
    }
}

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

#[derive(PartialEq)]
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
    names: Vec<String>,
    industries: Vec<String>,
    cities: Vec<String>,
    states: Vec<String>,
    countries: Vec<String>,
}

pub struct ChunkResult {
    pub data: Vec<u8>,
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
pub fn generate_chunk(
    start_id: u32,
    target_chunk_size: usize,
    pools: &DataPools,
    mut rng: ChaCha8Rng,
    pretty: bool,
    is_first: bool,
    format: &OutputFormat,
    mut progress: Arc<Mutex<ProgressInfo>>,
) -> ChunkResult {
    let mut output = Vec::with_capacity(target_chunk_size + 1024);
    let json_patterns = JsonPatterns::new();
    if is_first && *format == OutputFormat::CSV {
        output.extend_from_slice(b"id,name,industry,revenue,employees,city,state,country\n");
    } else if is_first && *format == OutputFormat::JSON {
        output.extend_from_slice(if pretty { b"[\n  " } else { b"[" });
    }

    let mut current_id = start_id;
    let mut is_start = is_first;

    while output.len() < target_chunk_size {
        let random_number = rng.gen_range(0..100);
        let location = BusinessLocation {
            id: current_id,
            name: pools.names[random_number].clone(),
            industry: pools.industries[random_number].clone().replace(",", ""),
            revenue: rng.gen_range(100000.0..100000000.0),
            employees: rng.gen_range(10..10000),
            city: pools.cities[random_number].clone(),
            state: pools.states[random_number].clone(),
            country: pools.countries[rng.gen_range(0..5)].clone(),
        };

        match format {
            OutputFormat::JSON => {
                write_location_json_simd(&location, &mut output, pretty, &json_patterns, is_start);
            }
            OutputFormat::CSV => {
                write_location_csv_simd(&location, &mut output);
            }
        }

        current_id += 1;
        is_start = false;

        unsafe {
            let progress_locked = Arc::get_mut_unchecked(&mut progress);
            progress_locked.force_unlock();
            progress_locked.get_mut().update(output.len());
        }

        if current_id % REFRESH_COUNT == 0 {
            progress.lock().print_progress();
        }
    }

    ChunkResult { data: output }
}

#[inline]
fn write_location_csv_simd(location: &BusinessLocation, output: &mut Vec<u8>) {
    // Pre-convert numbers to strings to know exact sizes
    let id_str = location.id.to_string();
    let revenue_str = location.revenue.to_string();
    let employees_str = location.employees.to_string();

    // Calculate total size needed
    let total_size = id_str.len()
        + location.name.len()
        + location.industry.len()
        + revenue_str.len()
        + employees_str.len()
        + location.city.len()
        + location.state.len()
        + location.country.len()
        + 8; // 8 commas/newline

    // Ensure capacity
    output.reserve(total_size);

    // Write fields with SIMD
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

    // Process 32 bytes at a time using SIMD
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

#[inline]
fn create_quoted_pattern(field_name: &[u8]) -> QuotedFieldPattern {
    let mut prefix = [0u8; 32];
    let mut suffix = [0u8; 32];

    // Format: "field_name": "
    prefix[0] = b'"';
    prefix[1..1 + field_name.len()].copy_from_slice(field_name);
    prefix[1 + field_name.len()..1 + field_name.len() + 4].copy_from_slice(b"\": \"");

    // Just the closing quote
    suffix[0] = b'"';

    QuotedFieldPattern {
        prefix,
        suffix,
        prefix_len: 1 + field_name.len() + 4,
        suffix_len: 1,
    }
}

#[inline]
fn create_unquoted_pattern(field_name: &[u8]) -> UnquotedFieldPattern {
    let mut prefix = [0u8; 32];

    // Format: "field_name":
    prefix[0] = b'"';
    prefix[1..1 + field_name.len()].copy_from_slice(field_name);
    prefix[1 + field_name.len()..1 + field_name.len() + 3].copy_from_slice(b"\": ");

    UnquotedFieldPattern {
        prefix,
        prefix_len: 1 + field_name.len() + 3,
    }
}

#[inline]
fn write_location_json_simd(
    location: &BusinessLocation,
    output: &mut Vec<u8>,
    pretty: bool,
    patterns: &JsonPatterns,
    is_first: bool,
) {
    // Add comma if not first object
    if !is_first {
        output.extend_from_slice(if pretty { b",\n  " } else { b"," });
    }

    // Pre-convert numbers to strings once
    let id_str = location.id.to_string();
    let revenue_str = location.revenue.to_string();
    let employees_str = location.employees.to_string();

    // Select patterns based on pretty flag
    let (separator, ending) = if pretty {
        (&patterns.separator_pretty, &patterns.ending_pretty)
    } else {
        (&patterns.separator_compact, &patterns.ending_compact)
    };

    // Initial brace and formatting
    output.push(b'{');

    // Write id (first field, no leading comma)
    if pretty {
        output.extend_from_slice(b"\n    ");
    }
    output.extend_from_slice(
        &patterns.unquoted_field_patterns[0].prefix
            [..patterns.unquoted_field_patterns[0].prefix_len],
    );
    output.extend_from_slice(id_str.as_bytes());

    // Write revenue
    output.extend_from_slice(&separator[..if pretty { 6 } else { 1 }]);
    output.extend_from_slice(
        &patterns.unquoted_field_patterns[1].prefix
            [..patterns.unquoted_field_patterns[1].prefix_len],
    );
    output.extend_from_slice(revenue_str.as_bytes());

    // Write employees
    output.extend_from_slice(&separator[..if pretty { 6 } else { 1 }]);
    output.extend_from_slice(
        &patterns.unquoted_field_patterns[2].prefix
            [..patterns.unquoted_field_patterns[2].prefix_len],
    );
    output.extend_from_slice(employees_str.as_bytes());

    // Write quoted fields (name, industry, city, state, country)
    let values = [
        &location.name,
        &location.industry,
        &location.city,
        &location.state,
        &location.country,
    ];

    for (pattern, value) in patterns.quoted_field_patterns.iter().zip(values.iter()) {
        output.extend_from_slice(&separator[..if pretty { 6 } else { 1 }]);
        output.extend_from_slice(&pattern.prefix[..pattern.prefix_len]);
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(&pattern.suffix[..pattern.suffix_len]);
    }

    // Closing brace and formatting
    output.extend_from_slice(&ending[..if pretty { 4 } else { 1 }]);
}

use noodles::bgzf;
use noodles::core::{Position, Region};
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

// In-memory variant record structure
#[derive(Debug, Clone, serde::Serialize)]
pub struct VariantRecord {
    pub chromosome: String,
    pub position: u64,
    pub id: String,
    pub reference: String,
    pub alternate: Vec<String>,
    pub quality: Option<f32>,
    pub filter: Vec<String>,
    pub info: HashMap<String, serde_json::Value>,
}

// VCF metadata structure extracted from header
#[derive(Debug, Clone, serde::Serialize)]
pub struct VcfMetadata {
    pub file_format: String,
    pub reference: Option<String>,
    pub contigs: Vec<ContigInfo>,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContigInfo {
    pub id: String,
}

// VCF index structure - uses tabix index for efficient queries
#[derive(Debug)]
pub struct VcfIndex {
    vcf_path: PathBuf,
    index: tabix::Index,
    header: vcf::Header,
}

impl VcfIndex {
    // Helper to get alternate chromosome name
    fn get_chromosome_variants(chromosome: &str) -> Vec<String> {
        let mut variants = vec![chromosome.to_string()];
        if let Some(stripped) = chromosome.strip_prefix("chr") {
            variants.push(stripped.to_string());
        } else {
            variants.push(format!("chr{}", chromosome));
        }
        variants
    }

    pub fn query_by_position(&self, chromosome: &str, position: u64) -> Vec<VariantRecord> {
        // Try both chromosome formats
        for chr_variant in Self::get_chromosome_variants(chromosome) {
            let results = query_indexed_region(&self.vcf_path, &self.index, &self.header, &chr_variant, position, position);
            if !results.is_empty() {
                return results;
            }
        }
        Vec::new()
    }

    pub fn query_by_region(&self, chromosome: &str, start: u64, end: u64) -> Vec<VariantRecord> {
        // Try both chromosome formats
        for chr_variant in Self::get_chromosome_variants(chromosome) {
            let results = query_indexed_region(&self.vcf_path, &self.index, &self.header, &chr_variant, start, end);
            if !results.is_empty() {
                return results;
            }
        }
        Vec::new()
    }

    pub fn query_by_id(&self, id: &str) -> Vec<VariantRecord> {
        // For ID queries, we need to scan all variants (no efficient index for IDs in tabix)
        // Fall back to full scan
        query_all_by_id(&self.vcf_path, &self.header, id)
    }

    pub fn get_metadata(&self) -> VcfMetadata {
        extract_metadata(&self.header)
    }
}

// Helper function to query indexed VCF by region
fn query_indexed_region(
    vcf_path: &PathBuf,
    index: &tabix::Index,
    header: &vcf::Header,
    chromosome: &str,
    start: u64,
    end: u64,
) -> Vec<VariantRecord> {
    let mut results = Vec::new();

    // Create region with Position types
    let start_pos = match Position::try_from(start as usize) {
        Ok(p) => p,
        Err(_) => return results,
    };
    let end_pos = match Position::try_from(end as usize) {
        Ok(p) => p,
        Err(_) => return results,
    };
    let region = Region::new(chromosome, start_pos..=end_pos);

    // Open VCF file and query
    let file = match File::open(vcf_path) {
        Ok(f) => f,
        Err(_) => return results,
    };

    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));

    let query_result = match reader.query(header, index, &region) {
        Ok(q) => q,
        Err(_) => return results,
    };

    for record in query_result.flatten() {
        if let Ok(variant) = parse_variant_record(&record, header) {
            results.push(variant);
        }
    }

    results
}

// Helper function to query all variants by ID (full scan)
fn query_all_by_id(vcf_path: &PathBuf, header: &vcf::Header, id: &str) -> Vec<VariantRecord> {
    let mut results = Vec::new();

    let file = match File::open(vcf_path) {
        Ok(f) => f,
        Err(_) => return results,
    };

    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));

    // Skip header (we already have it)
    let _ = reader.read_header();

    for record in reader.records().flatten() {
        if let Ok(variant) = parse_variant_record(&record, header) {
            if variant.id == id {
                results.push(variant);
            }
        }
    }

    results
}

// Helper function to extract metadata from VCF header
fn extract_metadata(header: &vcf::Header) -> VcfMetadata {
    // Extract file format version
    let file_format = format!("{:?}", header.file_format());

    // Try to extract reference genome from header
    // This is stored in the ##reference header line if present
    // The noodles API makes this complex, so we skip it for now
    let reference = None;

    // Extract contig information
    let contigs: Vec<ContigInfo> = header
        .contigs()
        .keys()
        .map(|id| ContigInfo {
            id: id.to_string(),
        })
        .collect();

    // Extract sample names
    let samples: Vec<String> = header
        .sample_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    VcfMetadata {
        file_format,
        reference,
        contigs,
        samples,
    }
}

// Helper function to convert debug-formatted info values to JSON
// Converts: Integer(123) -> 123, Float(1.23) -> 1.23, String("foo") -> "foo", etc.
fn convert_info_value(debug_str: &str) -> serde_json::Value {
    let s = debug_str;

    // Handle common patterns from noodles VCF library:
    // Integer(123) -> JSON number
    // Float(1.23) -> JSON number
    // String("foo") -> JSON string
    // Array([Ok(Some(1)), Ok(Some(2))]) -> JSON array
    // Flag -> JSON true

    if s == "Flag" {
        return serde_json::Value::Bool(true);
    }

    // Match Integer(value)
    if let Some(inner) = s.strip_prefix("Integer(").and_then(|s| s.strip_suffix(')')) {
        if let Ok(num) = inner.parse::<i64>() {
            return serde_json::Value::Number(num.into());
        }
    }

    // Match Float(value)
    if let Some(inner) = s.strip_prefix("Float(").and_then(|s| s.strip_suffix(')')) {
        if let Ok(num) = inner.parse::<f64>() {
            if let Some(json_num) = serde_json::Number::from_f64(num) {
                return serde_json::Value::Number(json_num);
            }
        }
    }

    // Match Character(value)
    if let Some(inner) = s.strip_prefix("Character(").and_then(|s| s.strip_suffix(')')) {
        return serde_json::Value::String(inner.trim_matches('\'').to_string());
    }

    // Match String("value")
    if let Some(inner) = s.strip_prefix("String(\"").and_then(|s| s.strip_suffix("\")")) {
        return serde_json::Value::String(inner.to_string());
    }

    // Match Array([...])
    if let Some(inner) = s.strip_prefix("Array([").and_then(|s| s.strip_suffix("])")) {
        // Extract Ok(Some(value)) patterns
        let values: Vec<serde_json::Value> = inner
            .split("), ")
            .filter_map(|part| {
                let part = part.trim_end_matches(')');
                if let Some(val_str) = part.strip_prefix("Ok(Some(") {
                    let val_str = val_str.trim_matches('"');
                    // Try to parse as number first, otherwise string
                    if let Ok(num) = val_str.parse::<i64>() {
                        return Some(serde_json::Value::Number(num.into()));
                    }
                    if let Ok(num) = val_str.parse::<f64>() {
                        if let Some(json_num) = serde_json::Number::from_f64(num) {
                            return Some(serde_json::Value::Number(json_num));
                        }
                    }
                    return Some(serde_json::Value::String(val_str.to_string()));
                }
                None
            })
            .collect();
        return serde_json::Value::Array(values);
    }

    // Fall back to string if no pattern matched
    serde_json::Value::String(s.to_string())
}

// Helper function to parse a VCF record into a VariantRecord
fn parse_variant_record(record: &vcf::Record, header: &vcf::Header) -> std::io::Result<VariantRecord> {
    Ok(VariantRecord {
        chromosome: record.reference_sequence_name().to_string(),
        position: usize::from(
            record
                .variant_start()
                .transpose()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing position"))?,
        ) as u64,
        id: record
            .ids()
            .iter()
            .next()
            .unwrap_or(".")
            .to_string(),
        reference: record.reference_bases().to_string(),
        alternate: record
            .alternate_bases()
            .iter()
            .map(|alt| alt.map(|a| a.to_string()).unwrap_or_else(|_| ".".to_string()))
            .collect(),
        quality: record
            .quality_score()
            .transpose()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        filter: record
            .filters()
            .iter(header)
            .filter_map(|f| f.ok())
            .map(|filter| filter.to_string())
            .collect(),
        info: record
            .info()
            .iter(header)
            .filter_map(|item| item.ok())
            .filter_map(|(key, value)| {
                if let Some(val) = value {
                    let debug_str = format!("{:?}", val);
                    let json_value = convert_info_value(&debug_str);
                    Some((key.to_string(), json_value))
                } else {
                    // Flag with no value - just the key is present
                    Some((key.to_string(), serde_json::Value::Bool(true)))
                }
            })
            .collect(),
    })
}

// Load and index VCF file
pub fn load_vcf(path: &PathBuf, debug: bool, save_index: bool) -> std::io::Result<VcfIndex> {
    // Check if a .tbi index file exists
    let tbi_path = PathBuf::from(format!("{}.tbi", path.display()));

    let tabix_index = if tbi_path.exists() {
        // Use existing tabix index
        if debug {
            eprintln!("Found tabix index: {}", tbi_path.display());
        }
        eprintln!("Loading VCF file with existing tabix index...");
        tabix::fs::read(&tbi_path)?
    } else {
        // Build tabix index on the fly
        eprintln!("No tabix index found. Building index...");
        let index = vcf::fs::index(path)?;
        eprintln!("Tabix index built successfully");

        // Try to save index to disk if requested
        if save_index {
            match save_index_to_disk(&index, &tbi_path, debug) {
                Ok(()) => eprintln!("Tabix index saved to {}", tbi_path.display()),
                Err(e) => {
                    eprintln!("Warning: Failed to save tabix index to disk: {}", e);
                    eprintln!("Continuing with in-memory index...");
                }
            }
        } else if debug {
            eprintln!("Skipping index save (--never-save-index flag set)");
        }

        index
    };

    // Read header only
    let mut vcf_reader = vcf::io::reader::Builder::default()
        .build_from_path(path)?;
    let header = vcf_reader.read_header()?;

    eprintln!("VCF loaded (indexed mode)");

    Ok(VcfIndex {
        vcf_path: path.clone(),
        index: tabix_index,
        header,
    })
}

// Helper function to atomically save index to disk
fn save_index_to_disk(index: &tabix::Index, tbi_path: &PathBuf, debug: bool) -> std::io::Result<()> {
    use std::fs;
    use std::io::BufWriter;

    // Create temporary file with .tmp extension
    let tmp_path = PathBuf::from(format!("{}.tmp", tbi_path.display()));

    if debug {
        eprintln!("Writing index to temporary file: {}", tmp_path.display());
    }

    // Write index to temporary file
    {
        let tmp_file = fs::File::create(&tmp_path)?;
        let mut writer = tabix::io::Writer::new(BufWriter::new(tmp_file));
        writer.write_index(index)?;
    }

    // Check again if .tbi file was created by another process (race condition)
    if tbi_path.exists() {
        if debug {
            eprintln!("Index file appeared during write, removing temporary file");
        }
        fs::remove_file(&tmp_path)?;
        return Ok(());
    }

    // Atomically rename temp file to final .tbi file
    fs::rename(&tmp_path, tbi_path)?;

    Ok(())
}

// Format variant as JSON string
pub fn format_variant(variant: &VariantRecord) -> String {
    serde_json::to_string(variant).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_variant(chromosome: &str, position: u64, id: &str, reference: &str, alternate: Vec<&str>) -> VariantRecord {
        let mut info = HashMap::new();
        info.insert("NS".to_string(), serde_json::Value::Number(3.into()));
        info.insert("DP".to_string(), serde_json::Value::Number(14.into()));
        info.insert("AF".to_string(), serde_json::Number::from_f64(0.5).map(serde_json::Value::Number).unwrap());

        VariantRecord {
            chromosome: chromosome.to_string(),
            position,
            id: id.to_string(),
            reference: reference.to_string(),
            alternate: alternate.iter().map(|s| s.to_string()).collect(),
            quality: Some(29.0),
            filter: vec!["PASS".to_string()],
            info,
        }
    }

    #[test]
    fn test_format_variant_basic() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let json = format_variant(&variant);

        assert!(json.contains(r#""chromosome":"20""#));
        assert!(json.contains(r#""position":14370"#));
        assert!(json.contains(r#""id":"rs6054257""#));
        assert!(json.contains(r#""reference":"G""#));
        assert!(json.contains(r#""alternate":["A"]"#));
        assert!(json.contains(r#""quality":29"#));
        assert!(json.contains(r#""filter":["PASS"]"#));
        assert!(json.contains(r#""NS":3"#));
        assert!(json.contains(r#""DP":14"#));
        assert!(json.contains(r#""AF":0.5"#));
    }

    #[test]
    fn test_format_variant_multiple_alternates() {
        let variant = create_test_variant("20", 1110696, "rs6040355", "A", vec!["G", "T"]);
        let json = format_variant(&variant);

        assert!(json.contains(r#""alternate":["G","T"]"#));
    }

    #[test]
    fn test_format_variant_no_quality() {
        let mut variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        variant.quality = None;
        let json = format_variant(&variant);

        assert!(json.contains(r#""quality":null"#));
    }
}

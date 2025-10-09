use noodles::bgzf;
use noodles::core::{Position, Region};
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::fs::File;
use std::path::PathBuf;

// In-memory variant record structure
#[derive(Debug, Clone)]
pub struct VariantRecord {
    pub chromosome: String,
    pub position: u64,
    pub id: String,
    pub reference: String,
    pub alternate: Vec<String>,
    pub quality: Option<f32>,
    pub filter: String,
    pub info: String,
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

// Helper function to clean up debug-formatted info values
// Converts: Array([Ok(Some(1))]) -> 1, Float(-0.549) -> -0.549, etc.
fn clean_info_value(debug_str: &str) -> String {
    let s = debug_str;

    // Handle common patterns:
    // Integer(123) -> 123
    // Float(1.23) -> 1.23
    // String("foo") -> foo
    // Array([Ok(Some(1)), Ok(Some(2))]) -> 1,2
    // Flag -> empty string

    if s == "Flag" {
        return String::new();
    }

    // Match Integer(value), Float(value), Character(value)
    if let Some(inner) = s.strip_prefix("Integer(").and_then(|s| s.strip_suffix(')')) {
        return inner.to_string();
    }
    if let Some(inner) = s.strip_prefix("Float(").and_then(|s| s.strip_suffix(')')) {
        return inner.to_string();
    }
    if let Some(inner) = s.strip_prefix("Character(").and_then(|s| s.strip_suffix(')')) {
        return inner.trim_matches('\'').to_string();
    }

    // Match String("value")
    if let Some(inner) = s.strip_prefix("String(\"").and_then(|s| s.strip_suffix("\")")) {
        return inner.to_string();
    }

    // Match Array([...])
    if let Some(inner) = s.strip_prefix("Array([").and_then(|s| s.strip_suffix("])")) {
        // Extract Ok(Some(value)) patterns
        let values: Vec<String> = inner
            .split("), ")
            .filter_map(|part| {
                // Match Ok(Some(value))
                let part = part.trim_end_matches(')');
                part.strip_prefix("Ok(Some(")
                    .map(|v| v.trim_matches('"').to_string())
            })
            .collect();
        return values.join(",");
    }

    // Fall back to original if no pattern matched
    s.to_string()
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
            .map(|f| f.map(|filter| filter.to_string()).unwrap_or_else(|_| "".to_string()))
            .collect::<Vec<_>>()
            .join(";"),
        info: record
            .info()
            .iter(header)
            .map(|item| {
                item.map(|(key, value)| if let Some(val) = value {
                    let debug_str = format!("{:?}", val);
                    let clean_value = clean_info_value(&debug_str);
                    if clean_value.is_empty() {
                        key.to_string() // For flags
                    } else {
                        format!("{}={}", key, clean_value)
                    }
                } else {
                    key.to_string()
                })
                .unwrap_or_else(|_| String::new())
            })
            .collect::<Vec<_>>()
            .join(";"),
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
    format!(
        r#"{{"chromosome": "{}", "position": {}, "id": "{}", "reference": "{}", "alternate": [{}], "quality": {}, "filter": "{}", "info": "{}"}}"#,
        variant.chromosome,
        variant.position,
        variant.id,
        variant.reference,
        variant
            .alternate
            .iter()
            .map(|a| format!(r#""{}""#, a))
            .collect::<Vec<_>>()
            .join(", "),
        variant
            .quality
            .map(|q| q.to_string())
            .unwrap_or_else(|| "null".to_string()),
        variant.filter,
        variant.info
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_variant(chromosome: &str, position: u64, id: &str, reference: &str, alternate: Vec<&str>) -> VariantRecord {
        VariantRecord {
            chromosome: chromosome.to_string(),
            position,
            id: id.to_string(),
            reference: reference.to_string(),
            alternate: alternate.iter().map(|s| s.to_string()).collect(),
            quality: Some(29.0),
            filter: "PASS".to_string(),
            info: "NS=3;DP=14;AF=0.5".to_string(),
        }
    }

    #[test]
    fn test_format_variant_basic() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let json = format_variant(&variant);

        assert!(json.contains(r#""chromosome": "20""#));
        assert!(json.contains(r#""position": 14370"#));
        assert!(json.contains(r#""id": "rs6054257""#));
        assert!(json.contains(r#""reference": "G""#));
        assert!(json.contains(r#""alternate": ["A"]"#));
        assert!(json.contains(r#""quality": 29"#));
        assert!(json.contains(r#""filter": "PASS""#));
    }

    #[test]
    fn test_format_variant_multiple_alternates() {
        let variant = create_test_variant("20", 1110696, "rs6040355", "A", vec!["G", "T"]);
        let json = format_variant(&variant);

        assert!(json.contains(r#""alternate": ["G", "T"]"#));
    }

    #[test]
    fn test_format_variant_no_quality() {
        let mut variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        variant.quality = None;
        let json = format_variant(&variant);

        assert!(json.contains(r#""quality": null"#));
    }
}

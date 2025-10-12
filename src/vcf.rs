use noodles::bgzf;
use noodles::core::{Position, Region};
use noodles::csi::BinningIndex;
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Mutex;

// Variant structure - used both internally and exposed via MCP responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct Variant {
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
    pub reference_genome: ReferenceGenomeInfo,
    pub contigs: Vec<ContigInfo>,
    pub samples: Vec<String>,
}

// Information about the reference genome build
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReferenceGenomeInfo {
    pub build: String,
    pub source: ReferenceGenomeSource,
}

// Source of reference genome information
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceGenomeSource {
    HeaderLine,
    InferredFromContigLengths,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContigInfo {
    pub id: String,
}

// VCF index structure - uses tabix index for efficient queries
pub struct VcfIndex {
    index: tabix::Index,
    header: vcf::Header,
    reader: Mutex<vcf::io::Reader<bgzf::io::Reader<File>>>,
    id_index: HashMap<String, Vec<(String, u64)>>, // ID -> [(chromosome, position)]
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

    // Get list of chromosomes present in the VCF file
    pub fn get_available_chromosomes(&self) -> Vec<String> {
        // Try to get chromosomes from VCF header contigs first
        let from_header: Vec<String> = self
            .header
            .contigs()
            .keys()
            .map(|k| k.to_string())
            .collect();

        if !from_header.is_empty() {
            return from_header;
        }

        // Fall back to tabix index if header has no contigs
        if let Some(header) = self.index.header() {
            header
                .reference_sequence_names()
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    // Check if a chromosome (or its variant) exists in the header
    fn find_matching_chromosome(&self, chromosome: &str) -> Option<String> {
        let variants = Self::get_chromosome_variants(chromosome);
        let available = self.get_available_chromosomes();

        variants
            .into_iter()
            .find(|variant| available.contains(variant))
    }

    pub fn query_by_position(
        &self,
        chromosome: &str,
        position: u64,
    ) -> (Vec<Variant>, Option<String>) {
        // Try to find the matching chromosome format
        if let Some(matching_chr) = self.find_matching_chromosome(chromosome) {
            let mut reader = self.reader.lock().unwrap();
            let results = query_indexed_region(
                &mut reader,
                &self.index,
                &self.header,
                &matching_chr,
                position,
                position,
            );
            return (results, Some(matching_chr));
        }
        (Vec::new(), None)
    }

    pub fn query_by_region(
        &self,
        chromosome: &str,
        start: u64,
        end: u64,
    ) -> (Vec<Variant>, Option<String>) {
        // Try to find the matching chromosome format
        if let Some(matching_chr) = self.find_matching_chromosome(chromosome) {
            let mut reader = self.reader.lock().unwrap();
            let results = query_indexed_region(
                &mut reader,
                &self.index,
                &self.header,
                &matching_chr,
                start,
                end,
            );
            return (results, Some(matching_chr));
        }
        (Vec::new(), None)
    }

    pub fn query_by_id(&self, id: &str) -> Vec<Variant> {
        // Use the ID index for O(1) lookup
        if let Some(locations) = self.id_index.get(id) {
            let mut results = Vec::new();
            let mut reader = self.reader.lock().unwrap();

            for (chromosome, position) in locations {
                let variants = query_indexed_region(
                    &mut reader,
                    &self.index,
                    &self.header,
                    chromosome,
                    *position,
                    *position,
                );
                results.extend(variants);
            }

            results
        } else {
            Vec::new()
        }
    }

    pub fn get_metadata(&self) -> VcfMetadata {
        extract_metadata(&self.header)
    }

    pub fn get_reference_genome(&self) -> String {
        let metadata = self.get_metadata();
        format!(
            "{} ({})",
            metadata.reference_genome.build,
            match metadata.reference_genome.source {
                ReferenceGenomeSource::HeaderLine => "from header",
                ReferenceGenomeSource::InferredFromContigLengths => "inferred from contigs",
                ReferenceGenomeSource::Unknown => "unknown source",
            }
        )
    }
}

// Helper function to query indexed VCF by region
fn query_indexed_region(
    reader: &mut vcf::io::Reader<bgzf::io::Reader<File>>,
    index: &tabix::Index,
    header: &vcf::Header,
    chromosome: &str,
    start: u64,
    end: u64,
) -> Vec<Variant> {
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

// Helper function to infer genome build from contig lengths
// GRCh37/hg19: chr1 = 249,250,621 bp
// GRCh38/hg38: chr1 = 248,956,422 bp
fn infer_genome_build_from_contigs(header: &vcf::Header) -> Option<String> {
    const CHR1_GRCH37_LENGTH: usize = 249_250_621;
    const CHR1_GRCH38_LENGTH: usize = 248_956_422;
    const TOLERANCE: usize = 1000; // Allow small differences

    // Try both "chr1" and "1" naming conventions
    for chr_name in ["chr1", "1"] {
        if let Some(contig) = header.contigs().get(chr_name) {
            if let Some(length) = contig.length() {
                let diff_grch37 =
                    (length as i64 - CHR1_GRCH37_LENGTH as i64).unsigned_abs() as usize;
                let diff_grch38 =
                    (length as i64 - CHR1_GRCH38_LENGTH as i64).unsigned_abs() as usize;

                if diff_grch37 < TOLERANCE {
                    return Some("GRCh37".to_string());
                } else if diff_grch38 < TOLERANCE {
                    return Some("GRCh38".to_string());
                }
            }
        }
    }

    None
}

// Helper function to extract reference genome from VCF header
fn extract_reference_genome(header: &vcf::Header) -> ReferenceGenomeInfo {
    use vcf::header::record::value::Collection;

    // Try to get ##reference line from header
    // Collection can be Unstructured (Vec<String>) or Structured (IndexMap)
    // ##reference is typically unstructured with a single string value
    if let Some(Collection::Unstructured(values)) = header.get("reference") {
        if let Some(reference_value) = values.first() {
            return ReferenceGenomeInfo {
                build: reference_value.clone(),
                source: ReferenceGenomeSource::HeaderLine,
            };
        }
    }

    // Fall back to inferring from contig lengths
    if let Some(inferred_build) = infer_genome_build_from_contigs(header) {
        return ReferenceGenomeInfo {
            build: inferred_build,
            source: ReferenceGenomeSource::InferredFromContigLengths,
        };
    }

    // Unknown
    ReferenceGenomeInfo {
        build: "Unknown".to_string(),
        source: ReferenceGenomeSource::Unknown,
    }
}

// Helper function to extract metadata from VCF header
fn extract_metadata(header: &vcf::Header) -> VcfMetadata {
    // Extract file format version
    let file_format = format!("{:?}", header.file_format());

    // Extract reference genome information
    let reference_genome = extract_reference_genome(header);

    // Extract contig information
    let contigs: Vec<ContigInfo> = header
        .contigs()
        .keys()
        .map(|id| ContigInfo { id: id.to_string() })
        .collect();

    // Extract sample names
    let samples: Vec<String> = header
        .sample_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    VcfMetadata {
        file_format,
        reference_genome,
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
    if let Some(inner) = s
        .strip_prefix("Character(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return serde_json::Value::String(inner.trim_matches('\'').to_string());
    }

    // Match String("value")
    if let Some(inner) = s
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
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

// Helper function to parse a VCF record into a Variant
fn parse_variant_record(record: &vcf::Record, header: &vcf::Header) -> std::io::Result<Variant> {
    Ok(Variant {
        chromosome: record.reference_sequence_name().to_string(),
        position: usize::from(
            record
                .variant_start()
                .transpose()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing position")
                })?,
        ) as u64,
        id: record.ids().iter().next().unwrap_or(".").to_string(),
        reference: record.reference_bases().to_string(),
        alternate: record
            .alternate_bases()
            .iter()
            .map(|alt| {
                alt.map(|a| a.to_string())
                    .unwrap_or_else(|_| ".".to_string())
            })
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
            .map(|item| {
                item.map(|(key, value)| {
                    if let Some(val) = value {
                        let debug_str = format!("{:?}", val);
                        let json_value = convert_info_value(&debug_str);
                        (key.to_string(), json_value)
                    } else {
                        // Flag with no value - just the key is present
                        (key.to_string(), serde_json::Value::Bool(true))
                    }
                })
            })
            .filter_map(|item| item.ok())
            .collect(),
    })
}

// Helper function to save ID index to disk
fn save_id_index_to_disk(
    id_index: &HashMap<String, Vec<(String, u64)>>,
    idx_path: &PathBuf,
    debug: bool,
) -> std::io::Result<()> {
    use std::fs;
    use std::io::Write;

    // Create temporary file with .tmp extension
    let tmp_path = PathBuf::from(format!("{}.tmp", idx_path.display()));

    if debug {
        eprintln!("Writing ID index to temporary file: {}", tmp_path.display());
    }

    // Serialize and write to temp file
    {
        let encoded = bincode::serialize(id_index)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut tmp_file = fs::File::create(&tmp_path)?;
        tmp_file.write_all(&encoded)?;
    }

    // Check if .idx file was created by another process (race condition)
    if idx_path.exists() {
        if debug {
            eprintln!("ID index file appeared during write, removing temporary file");
        }
        fs::remove_file(&tmp_path)?;
        return Ok(());
    }

    // Atomically rename temp file to final .idx file
    fs::rename(&tmp_path, idx_path)?;

    Ok(())
}

// Helper function to load ID index from disk
fn load_id_index_from_disk(
    idx_path: &PathBuf,
    debug: bool,
) -> std::io::Result<HashMap<String, Vec<(String, u64)>>> {
    use std::fs;
    use std::io::Read;

    if debug {
        eprintln!("Loading ID index from: {}", idx_path.display());
    }

    let mut file = fs::File::open(idx_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let id_index: HashMap<String, Vec<(String, u64)>> = bincode::deserialize(&buffer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(id_index)
}

// Helper function to build ID index by scanning all variants
fn build_id_index(
    path: &PathBuf,
    header: &vcf::Header,
    debug: bool,
) -> std::io::Result<HashMap<String, Vec<(String, u64)>>> {
    let mut id_index: HashMap<String, Vec<(String, u64)>> = HashMap::new();

    if debug {
        eprintln!("Building ID index...");
    }

    let file = File::open(path)?;
    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));
    let _ = reader.read_header()?; // Skip header

    let mut count = 0;
    for record in reader.records().flatten() {
        if let Ok(variant) = parse_variant_record(&record, header) {
            // Skip "." (missing ID)
            if variant.id != "." {
                id_index
                    .entry(variant.id.clone())
                    .or_default()
                    .push((variant.chromosome.clone(), variant.position));
            }
            count += 1;
        }
    }

    if debug {
        eprintln!(
            "ID index built: {} variants scanned, {} unique IDs indexed",
            count,
            id_index.len()
        );
    } else {
        eprintln!("ID index built ({} unique IDs)", id_index.len());
    }

    Ok(id_index)
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

    // Create reader for queries
    let file = File::open(path)?;
    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));
    let header = reader.read_header()?;

    // Check if ID index file exists
    let idx_path = PathBuf::from(format!("{}.idx", path.display()));

    let id_index = if idx_path.exists() {
        // Load existing ID index
        if debug {
            eprintln!("Found ID index: {}", idx_path.display());
        }
        eprintln!("Loading VCF file with existing ID index...");
        match load_id_index_from_disk(&idx_path, debug) {
            Ok(index) => {
                eprintln!("ID index loaded ({} unique IDs)", index.len());
                index
            }
            Err(e) => {
                eprintln!("Warning: Failed to load ID index: {}", e);
                eprintln!("Rebuilding ID index...");
                let index = build_id_index(path, &header, debug)?;

                // Try to save the rebuilt index
                if save_index {
                    match save_id_index_to_disk(&index, &idx_path, debug) {
                        Ok(()) => eprintln!("ID index saved to {}", idx_path.display()),
                        Err(e) => {
                            eprintln!("Warning: Failed to save ID index: {}", e);
                            eprintln!("Continuing with in-memory index...");
                        }
                    }
                }

                index
            }
        }
    } else {
        // Build ID index from scratch
        let index = build_id_index(path, &header, debug)?;

        // Try to save index to disk if requested
        if save_index {
            match save_id_index_to_disk(&index, &idx_path, debug) {
                Ok(()) => eprintln!("ID index saved to {}", idx_path.display()),
                Err(e) => {
                    eprintln!("Warning: Failed to save ID index to disk: {}", e);
                    eprintln!("Continuing with in-memory index...");
                }
            }
        } else if debug {
            eprintln!("Skipping ID index save (--never-save-index flag set)");
        }

        index
    };

    eprintln!("VCF loaded (indexed mode)");

    Ok(VcfIndex {
        index: tabix_index,
        header,
        reader: Mutex::new(reader),
        id_index,
    })
}

// Helper function to atomically save index to disk
fn save_index_to_disk(
    index: &tabix::Index,
    tbi_path: &PathBuf,
    debug: bool,
) -> std::io::Result<()> {
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

// Format variant for MCP response (no-op now that types are unified)
pub fn format_variant(variant: Variant) -> Variant {
    variant
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_variant(
        chromosome: &str,
        position: u64,
        id: &str,
        reference: &str,
        alternate: Vec<&str>,
    ) -> Variant {
        let mut info = HashMap::new();
        info.insert("NS".to_string(), serde_json::Value::Number(3.into()));
        info.insert("DP".to_string(), serde_json::Value::Number(14.into()));
        info.insert(
            "AF".to_string(),
            serde_json::Number::from_f64(0.5)
                .map(serde_json::Value::Number)
                .unwrap(),
        );

        Variant {
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
        let dto = format_variant(variant.clone());

        assert_eq!(dto.chromosome, "20");
        assert_eq!(dto.position, 14370);
        assert_eq!(dto.id, "rs6054257");
        assert_eq!(dto.reference, "G");
        assert_eq!(dto.alternate, vec!["A"]);
        assert_eq!(dto.quality, Some(29.0));
        assert_eq!(dto.filter, vec!["PASS"]);
        assert_eq!(dto.info.get("NS"), Some(&serde_json::json!(3)));
        assert_eq!(dto.info.get("DP"), Some(&serde_json::json!(14)));
        assert_eq!(dto.info.get("AF"), Some(&serde_json::json!(0.5)));
    }

    #[test]
    fn test_format_variant_multiple_alternates() {
        let variant = create_test_variant("20", 1110696, "rs6040355", "A", vec!["G", "T"]);
        let dto = format_variant(variant.clone());

        assert_eq!(dto.alternate, vec!["G", "T"]);
    }

    #[test]
    fn test_format_variant_no_quality() {
        let mut variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        variant.quality = None;
        let dto = format_variant(variant);

        assert!(dto.quality.is_none());
    }
}

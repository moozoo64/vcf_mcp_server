use noodles::bgzf;
use noodles::core::{Position, Region};
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::collections::HashMap;
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

// VCF index structure - supports both tabix-indexed and in-memory modes
#[derive(Debug)]
pub enum VcfIndex {
    // Tabix-indexed mode: uses .tbi file for efficient region queries
    Indexed {
        vcf_path: PathBuf,
        index: tabix::Index,
        header: vcf::Header,
    },
    // In-memory mode: loads all variants into memory
    InMemory {
        position_index: HashMap<String, Vec<(u64, VariantRecord)>>,
        id_index: HashMap<String, Vec<VariantRecord>>,
        header: vcf::Header,
    },
}

impl VcfIndex {
    fn new_in_memory() -> Self {
        VcfIndex::InMemory {
            position_index: HashMap::new(),
            id_index: HashMap::new(),
            header: vcf::Header::default(),
        }
    }

    fn add_variant(&mut self, variant: VariantRecord) {
        if let VcfIndex::InMemory { position_index, id_index, .. } = self {
            // Add to position index
            position_index
                .entry(variant.chromosome.clone())
                .or_default()
                .push((variant.position, variant.clone()));

            // Add to ID index if ID is not '.'
            if variant.id != "." {
                id_index
                    .entry(variant.id.clone())
                    .or_default()
                    .push(variant);
            }
        }
    }

    fn finalize(&mut self) {
        if let VcfIndex::InMemory { position_index, .. } = self {
            // Sort position indexes
            for variants in position_index.values_mut() {
                variants.sort_by_key(|(pos, _)| *pos);
            }
        }
    }

    pub fn query_by_position(&self, chromosome: &str, position: u64) -> Vec<VariantRecord> {
        match self {
            VcfIndex::Indexed { vcf_path, index, header } => {
                // Use tabix index to query region
                query_indexed_region(vcf_path, index, header, chromosome, position, position)
            }
            VcfIndex::InMemory { position_index, .. } => {
                position_index
                    .get(chromosome)
                    .map(|variants| {
                        variants
                            .iter()
                            .filter(|(pos, _)| *pos == position)
                            .map(|(_, variant)| variant.clone())
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }

    pub fn query_by_region(&self, chromosome: &str, start: u64, end: u64) -> Vec<VariantRecord> {
        match self {
            VcfIndex::Indexed { vcf_path, index, header } => {
                // Use tabix index to query region
                query_indexed_region(vcf_path, index, header, chromosome, start, end)
            }
            VcfIndex::InMemory { position_index, .. } => {
                position_index
                    .get(chromosome)
                    .map(|variants| {
                        variants
                            .iter()
                            .filter(|(pos, _)| *pos >= start && *pos <= end)
                            .map(|(_, variant)| variant.clone())
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }

    pub fn query_by_id(&self, id: &str) -> Vec<VariantRecord> {
        match self {
            VcfIndex::Indexed { vcf_path, index: _, header } => {
                // For ID queries, we need to scan all variants (no efficient index for IDs in tabix)
                // Fall back to full scan
                query_all_by_id(vcf_path, header, id)
            }
            VcfIndex::InMemory { id_index, .. } => {
                id_index
                    .get(id)
                    .map(|variants| variants.clone())
                    .unwrap_or_default()
            }
        }
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

    for result in query_result {
        if let Ok(record) = result {
            if let Ok(variant) = parse_variant_record(&record, header) {
                results.push(variant);
            }
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

    for result in reader.records() {
        if let Ok(record) = result {
            if let Ok(variant) = parse_variant_record(&record, header) {
                if variant.id == id {
                    results.push(variant);
                }
            }
        }
    }

    results
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
                    format!("{}={:?}", key, val)
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
pub fn load_vcf(path: &PathBuf) -> std::io::Result<VcfIndex> {
    // Check if a .tbi index file exists
    let tbi_path = PathBuf::from(format!("{}.tbi", path.display()));

    if tbi_path.exists() {
        // Use tabix-indexed mode
        println!("Found tabix index: {}", tbi_path.display());
        println!("Loading VCF file with tabix index: {}", path.display());

        let tabix_index = tabix::fs::read(&tbi_path)?;

        // Read header only
        let mut vcf_reader = vcf::io::reader::Builder::default()
            .build_from_path(path)?;
        let header = vcf_reader.read_header()?;

        println!("VCF loaded in indexed mode (using .tbi file)");

        Ok(VcfIndex::Indexed {
            vcf_path: path.clone(),
            index: tabix_index,
            header,
        })
    } else {
        // Use in-memory mode (load all variants)
        println!("No tabix index found, loading VCF file into memory: {}", path.display());

        let mut index = VcfIndex::new_in_memory();

        let mut vcf_reader = vcf::io::reader::Builder::default()
            .build_from_path(path)?;

        // Read header
        let header = vcf_reader.read_header()?;
        if let VcfIndex::InMemory { header: h, .. } = &mut index {
            *h = header.clone();
        }

        let mut line_number = 0;
        for result in vcf_reader.records() {
            let record = result?;
            line_number += 1;

            let variant = parse_variant_record(&record, &header)?;
            index.add_variant(variant);

            if line_number % 1000 == 0 {
                println!("Indexed {} variants...", line_number);
            }
        }

        index.finalize();
        println!("Finished indexing {} variants", line_number);

        Ok(index)
    }
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
    fn test_query_by_position_exact_match() {
        let mut index = VcfIndex::new_in_memory();
        let variant1 = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let variant2 = create_test_variant("20", 17330, "rs6040355", "T", vec!["A"]);

        index.add_variant(variant1);
        index.add_variant(variant2);
        index.finalize();

        let results = index.query_by_position("20", 14370);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rs6054257");
        assert_eq!(results[0].position, 14370);
    }

    #[test]
    fn test_query_by_position_no_match() {
        let mut index = VcfIndex::new_in_memory();
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);

        index.add_variant(variant);
        index.finalize();

        let results = index.query_by_position("20", 99999);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_position_different_chromosome() {
        let mut index = VcfIndex::new_in_memory();
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);

        index.add_variant(variant);
        index.finalize();

        let results = index.query_by_position("X", 14370);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_region() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.add_variant(create_test_variant("20", 17330, "rs6040355", "T", vec!["A"]));
        index.add_variant(create_test_variant("20", 1110696, "rs6040356", "A", vec!["G", "T"]));
        index.finalize();

        let results = index.query_by_region("20", 14000, 18000);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].position, 14370);
        assert_eq!(results[1].position, 17330);
    }

    #[test]
    fn test_query_by_region_no_matches() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.finalize();

        let results = index.query_by_region("20", 100000, 200000);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_region_boundary() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.finalize();

        // Test inclusive boundaries
        let results = index.query_by_region("20", 14370, 14370);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_by_id() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.add_variant(create_test_variant("20", 17330, "rs6040355", "T", vec!["A"]));
        index.finalize();

        let results = index.query_by_id("rs6054257");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chromosome, "20");
        assert_eq!(results[0].position, 14370);
    }

    #[test]
    fn test_query_by_id_no_match() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.finalize();

        let results = index.query_by_id("rs99999999");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_id_dot_not_indexed() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, ".", "G", vec!["A"]));
        index.finalize();

        // Variants with ID "." should not be indexed by ID
        let results = index.query_by_id(".");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_multiple_variants_same_id() {
        let mut index = VcfIndex::new_in_memory();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.add_variant(create_test_variant("20", 17330, "rs6054257", "T", vec!["A"]));
        index.finalize();

        let results = index.query_by_id("rs6054257");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_finalize_sorts_positions() {
        let mut index = VcfIndex::new_in_memory();
        // Add variants out of order
        index.add_variant(create_test_variant("20", 1110696, "rs3", "A", vec!["G"]));
        index.add_variant(create_test_variant("20", 14370, "rs1", "G", vec!["A"]));
        index.add_variant(create_test_variant("20", 17330, "rs2", "T", vec!["A"]));
        index.finalize();

        let results = index.query_by_region("20", 0, 2000000);
        assert_eq!(results.len(), 3);
        // Check they're returned in sorted order
        assert_eq!(results[0].position, 14370);
        assert_eq!(results[1].position, 17330);
        assert_eq!(results[2].position, 1110696);
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

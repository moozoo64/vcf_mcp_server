use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::collections::HashMap;
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

// In-memory index structure
#[derive(Debug, Clone)]
pub struct VcfIndex {
    // Position-based index: chromosome -> sorted list of (position, variant)
    position_index: HashMap<String, Vec<(u64, VariantRecord)>>,
    // ID-based index: variant ID -> variant record
    id_index: HashMap<String, Vec<VariantRecord>>,
    // Store the VCF header for reference
    header: vcf::Header,
}

impl VcfIndex {
    fn new() -> Self {
        VcfIndex {
            position_index: HashMap::new(),
            id_index: HashMap::new(),
            header: vcf::Header::default(),
        }
    }

    fn add_variant(&mut self, variant: VariantRecord) {
        // Add to position index
        self.position_index
            .entry(variant.chromosome.clone())
            .or_default()
            .push((variant.position, variant.clone()));

        // Add to ID index if ID is not '.'
        if variant.id != "." {
            self.id_index
                .entry(variant.id.clone())
                .or_default()
                .push(variant);
        }
    }

    fn finalize(&mut self) {
        // Sort position indexes
        for variants in self.position_index.values_mut() {
            variants.sort_by_key(|(pos, _)| *pos);
        }
    }

    pub fn query_by_position(&self, chromosome: &str, position: u64) -> Vec<&VariantRecord> {
        self.position_index
            .get(chromosome)
            .map(|variants| {
                variants
                    .iter()
                    .filter(|(pos, _)| *pos == position)
                    .map(|(_, variant)| variant)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn query_by_region(&self, chromosome: &str, start: u64, end: u64) -> Vec<&VariantRecord> {
        self.position_index
            .get(chromosome)
            .map(|variants| {
                variants
                    .iter()
                    .filter(|(pos, _)| *pos >= start && *pos <= end)
                    .map(|(_, variant)| variant)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn query_by_id(&self, id: &str) -> Vec<&VariantRecord> {
        self.id_index
            .get(id)
            .map(|variants| variants.iter().collect())
            .unwrap_or_default()
    }
}

// Load and index VCF file
pub fn load_vcf(path: &PathBuf) -> std::io::Result<VcfIndex> {
    let mut index = VcfIndex::new();

    let mut vcf_reader = vcf::io::reader::Builder::default()
        .build_from_path(path)?;

    // Read header
    let header = vcf_reader.read_header()?;
    index.header = header.clone();

    println!("Loading VCF file: {}", path.display());

    let mut line_number = 0;
    for result in vcf_reader.records() {
        let record = result?;
        line_number += 1;

        let variant = VariantRecord {
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
                .iter(&header)
                .map(|f| f.map(|filter| filter.to_string()).unwrap_or_else(|_| "".to_string()))
                .collect::<Vec<_>>()
                .join(";"),
            info: record
                .info()
                .iter(&header)
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
        };

        index.add_variant(variant);

        if line_number % 1000 == 0 {
            println!("Indexed {} variants...", line_number);
        }
    }

    index.finalize();
    println!("Finished indexing {} variants", line_number);

    Ok(index)
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
        let mut index = VcfIndex::new();
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
        let mut index = VcfIndex::new();
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);

        index.add_variant(variant);
        index.finalize();

        let results = index.query_by_position("20", 99999);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_position_different_chromosome() {
        let mut index = VcfIndex::new();
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);

        index.add_variant(variant);
        index.finalize();

        let results = index.query_by_position("X", 14370);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_region() {
        let mut index = VcfIndex::new();
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
        let mut index = VcfIndex::new();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.finalize();

        let results = index.query_by_region("20", 100000, 200000);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_region_boundary() {
        let mut index = VcfIndex::new();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.finalize();

        // Test inclusive boundaries
        let results = index.query_by_region("20", 14370, 14370);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_by_id() {
        let mut index = VcfIndex::new();
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
        let mut index = VcfIndex::new();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.finalize();

        let results = index.query_by_id("rs99999999");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_by_id_dot_not_indexed() {
        let mut index = VcfIndex::new();
        index.add_variant(create_test_variant("20", 14370, ".", "G", vec!["A"]));
        index.finalize();

        // Variants with ID "." should not be indexed by ID
        let results = index.query_by_id(".");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_multiple_variants_same_id() {
        let mut index = VcfIndex::new();
        index.add_variant(create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]));
        index.add_variant(create_test_variant("20", 17330, "rs6054257", "T", vec!["A"]));
        index.finalize();

        let results = index.query_by_id("rs6054257");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_finalize_sorts_positions() {
        let mut index = VcfIndex::new();
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

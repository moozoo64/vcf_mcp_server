use std::path::PathBuf;
use vcf_mcp_server::vcf::{load_vcf, format_variant};

#[test]
fn test_load_compressed_vcf() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    // Skip test if sample file doesn't exist
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path).expect("Failed to load VCF file");

    // Query by position - should find rs6054257 at 20:14370
    let results = index.query_by_position("20", 14370);
    assert_eq!(results.len(), 1, "Should find exactly one variant at 20:14370");
    assert_eq!(results[0].id, "rs6054257");
    assert_eq!(results[0].reference, "G");
    assert_eq!(results[0].alternate, vec!["A"]);
}

#[test]
fn test_query_region_with_real_data() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path).expect("Failed to load VCF file");

    // Query region 20:14000-18000 should find variants at 14370 and 17330
    let results = index.query_by_region("20", 14000, 18000);
    assert_eq!(results.len(), 2, "Should find 2 variants in region 20:14000-18000");

    // Verify they're in sorted order
    assert_eq!(results[0].position, 14370);
    assert_eq!(results[1].position, 17330);
}

#[test]
fn test_query_by_id_with_real_data() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path).expect("Failed to load VCF file");

    // Query by rs6040355 - should find variant with multiple alternates
    let results = index.query_by_id("rs6040355");
    assert_eq!(results.len(), 1, "Should find exactly one variant with ID rs6040355");
    assert_eq!(results[0].chromosome, "20");
    assert_eq!(results[0].position, 1110696);
    assert_eq!(results[0].alternate.len(), 2, "rs6040355 should have 2 alternate alleles");
}

#[test]
fn test_format_variant_with_real_data() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path).expect("Failed to load VCF file");
    let results = index.query_by_position("20", 14370);

    assert!(!results.is_empty(), "Should find variant at 20:14370");

    let json = format_variant(results[0]);

    // Verify JSON contains expected fields
    assert!(json.contains(r#""chromosome": "20""#));
    assert!(json.contains(r#""position": 14370"#));
    assert!(json.contains(r#""id": "rs6054257""#));
    assert!(json.contains(r#""reference": "G""#));
    assert!(json.contains(r#""alternate": ["A"]"#));
}

#[test]
fn test_load_nonexistent_file() {
    let vcf_path = PathBuf::from("nonexistent.vcf.gz");
    let result = load_vcf(&vcf_path);

    assert!(result.is_err(), "Loading nonexistent file should return an error");
}

#[test]
fn test_chromosome_x_variant() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path).expect("Failed to load VCF file");

    // Query for variant on chromosome X at position 10
    let results = index.query_by_position("X", 10);
    assert_eq!(results.len(), 1, "Should find variant at X:10");
    assert_eq!(results[0].id, "rsTest");
}

#[test]
fn test_microsat_variant() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path).expect("Failed to load VCF file");

    // Query for microsat1 variant
    let results = index.query_by_id("microsat1");
    assert_eq!(results.len(), 1, "Should find microsat1 variant");
    assert_eq!(results[0].chromosome, "20");
    assert_eq!(results[0].position, 1234567);
}

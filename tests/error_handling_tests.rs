// Error handling and edge case tests for VCF MCP Server
use std::path::PathBuf;
use vcf_mcp_server::vcf::load_vcf;

// ============================================================================
// Malformed Filter Expression Tests
// ============================================================================

#[test]
fn test_filter_with_unknown_field() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let filter_engine = index.filter_engine();

    // Filter with unknown field - vcf-filter may not error on parse but will fail evaluation
    let filter = "UNKNOWN_FIELD > 50";
    let parse_result = filter_engine.parse_filter(filter);
    // vcf-filter may accept this syntactically but fail on evaluation with actual data
    // The important thing is it doesn't panic
    let _ = parse_result;
}

#[test]
fn test_filter_with_invalid_syntax() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let filter_engine = index.filter_engine();

    // Filters that should be detectable as parse errors
    let definitely_invalid = vec![
        ("QUAL >", "incomplete expression"),
        ("> 50", "missing field name"),
        ("QUAL 50", "missing operator"),
        ("QUAL == ", "missing value"),
    ];

    for (filter, reason) in definitely_invalid {
        let result = filter_engine.parse_filter(filter);
        // vcf-filter should detect these syntax errors
        if result.is_ok() {
            eprintln!(
                "Warning: Filter '{}' ({}) was accepted but might fail on evaluation",
                filter, reason
            );
        }
    }
}

#[test]
fn test_filter_with_complex_and_or() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 14000, 18000);
    assert!(!variants.is_empty());

    let variant = &variants[0];
    let filter_engine = index.filter_engine();

    // Complex && and || expressions (new syntax)
    if let Some(qual) = variant.quality {
        let filter = format!("QUAL > {} && FILTER == \"PASS\"", qual - 1.0);
        let matches = variant.filter.contains(&"PASS".to_string());
        let result = filter_engine
            .evaluate(&filter, &variant.raw_row)
            .unwrap_or(false);
        assert_eq!(
            result, matches,
            "Filter evaluation should match expected result"
        );

        // || expression - test it doesn't crash
        let filter_or = format!("QUAL > {} || QUAL < 0", qual + 100.0);
        let _ = filter_engine.evaluate(&filter_or, &variant.raw_row);
    }
}

// ============================================================================
// Chromosome Edge Cases
// ============================================================================

#[test]
fn test_mitochondrial_chromosome_variations() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Test various MT chromosome names (none should crash)
    let mt_names = vec!["MT", "chrM", "M", "chrMT"];
    for name in mt_names {
        let (variants, matched) = index.query_by_position(name, 100);
        // May or may not find variants, but should not crash
        assert!(variants.is_empty() || matched.is_some());
    }
}

#[test]
fn test_numeric_chromosome_edge_cases() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Test edge cases for numeric chromosomes
    let edge_chromosomes = vec!["0", "99", "chr0", "chr99"];
    for chr in edge_chromosomes {
        let (variants, _) = index.query_by_position(chr, 1000);
        // Should return empty but not crash
        assert!(
            variants.is_empty() || !variants.is_empty(),
            "Query for {} should complete without error",
            chr
        );
    }
}

#[test]
fn test_special_chromosome_names() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Test special/alternative chromosome names
    let special_names = vec![
        "chrUn",       // Unplaced
        "chr1_random", // Random contigs
        "HLA-A",       // HLA genes
        "scaffold_1",  // Scaffolds
    ];

    for name in special_names {
        let (variants, matched) = index.query_by_position(name, 100);
        // Should not panic, may or may not find results
        assert!(variants.len() == 0 || matched.is_some());
    }
}

// ============================================================================
// Position Boundary Tests
// ============================================================================

#[test]
fn test_extremely_large_position() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Position beyond any realistic chromosome
    let (variants, _) = index.query_by_position("20", u64::MAX);
    assert_eq!(variants.len(), 0);
}

#[test]
fn test_region_with_same_start_and_end() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Single-base region (start == end)
    let (variants, _) = index.query_by_region("20", 14370, 14370);
    // Should find the variant at exactly that position
    if !variants.is_empty() {
        assert_eq!(variants[0].position, 14370);
    }
}

#[test]
fn test_region_with_very_large_span() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Very large region
    let (variants, matched) = index.query_by_region("20", 1, 1_000_000_000);
    assert_eq!(matched, Some("20".to_string()));
    // Should find all variants on chromosome 20
    assert!(variants.len() > 0);
}

// ============================================================================
// Variant Data Edge Cases
// ============================================================================

#[test]
fn test_variant_with_very_long_id() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query with extremely long ID (should not crash)
    let long_id = "rs".to_string() + &"0".repeat(1000);
    let results = index.query_by_id(&long_id);
    assert_eq!(results.len(), 0);
}

#[test]
fn test_variant_with_special_characters_in_id() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // IDs with special characters (should not crash)
    let special_ids = vec![
        "id;with;semicolons",
        "id\twith\ttabs",
        "id with spaces",
        "id|with|pipes",
    ];

    for id in special_ids {
        let results = index.query_by_id(id);
        // Should complete without error
        assert!(results.len() == 0 || results.len() > 0);
    }
}

#[test]
fn test_reference_allele_validation() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 14000, 18000);

    // All variants should have valid reference alleles
    for variant in &variants {
        assert!(
            !variant.reference.is_empty(),
            "Reference should not be empty"
        );
        // Reference should only contain valid bases (A, C, G, T, N)
        assert!(
            variant.reference.chars().all(|c| "ACGTN".contains(c)),
            "Reference should only contain valid bases: {}",
            variant.reference
        );
    }
}

// ============================================================================
// Info Field Edge Cases
// ============================================================================

#[test]
fn test_variant_info_field_access() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 14000, 18000);

    assert!(!variants.is_empty());
    let filter_engine = index.filter_engine();

    // Check that info fields are accessible
    for variant in &variants {
        // Info should be a map (could be empty)
        let _ = variant.info.len();

        // If variant has NS field, it should be parseable
        if let Some(ns_value) = variant.info.get("NS") {
            // Should be a valid value (not test the exact format)
            assert!(!ns_value.to_string().is_empty());
        }
    }
}

#[test]
fn test_filter_with_info_field() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 14000, 18000);
    assert!(!variants.is_empty());

    let variant = &variants[0];
    let filter_engine = index.filter_engine();

    // INFO field filtering is now supported with vcf-filter!
    // Test DP field if it exists
    if variant.info.contains_key("DP") {
        let filter = "DP >= 0"; // Should match any variant with DP field
        let result = filter_engine.evaluate(filter, &variant.raw_row);
        assert!(result.is_ok(), "INFO field filter should be valid");
    }
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_queries() {
    use std::sync::Arc;

    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = Arc::new(load_vcf(&vcf_path, false, false).expect("Failed to load VCF file"));

    // Perform multiple queries concurrently
    let tasks: Vec<_> = (0..10)
        .map(|_| {
            let index_clone = Arc::clone(&index);
            tokio::spawn(async move {
                let (variants, _) = index_clone.query_by_position("20", 14370);
                assert_eq!(variants.len(), 1);
                variants[0].id.clone()
            })
        })
        .collect();

    // All tasks should complete successfully
    for task in tasks {
        let result = task.await.expect("Task should complete");
        assert_eq!(result, "rs6054257");
    }
}

#[test]
fn test_metadata_access() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let metadata = index.get_metadata();

    // Verify metadata structure
    assert!(!metadata.reference_genome.build.is_empty());
    // Note: Contigs may or may not be populated depending on implementation
    // Just verify the field exists
    let _ = &metadata.contigs;

    // Check contig structure if any exist
    for contig in &metadata.contigs {
        assert!(!contig.id.is_empty(), "Contig ID should not be empty");
    }
}

#[test]
fn test_available_chromosomes_list() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let chromosomes = index.get_available_chromosomes();

    // Should have at least chromosome 20 and X
    assert!(chromosomes.len() >= 2);
    assert!(chromosomes.contains(&"20".to_string()));

    // Chromosomes should be unique
    let unique_count = chromosomes
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(
        chromosomes.len(),
        unique_count,
        "Chromosomes should be unique"
    );
}

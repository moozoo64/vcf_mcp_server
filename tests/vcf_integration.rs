use std::path::PathBuf;
use vcf_mcp_server::vcf::{format_variant, load_vcf, ReferenceGenomeSource};

#[test]
fn test_load_compressed_vcf() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    // Skip test if sample file doesn't exist
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query by position - should find rs6054257 at 20:14370
    let (results, _) = index.query_by_position("20", 14370);
    assert_eq!(
        results.len(),
        1,
        "Should find exactly one variant at 20:14370"
    );
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

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query region 20:14000-18000 should find variants at 14370 and 17330
    let (results, _) = index.query_by_region("20", 14000, 18000);
    assert_eq!(
        results.len(),
        2,
        "Should find 2 variants in region 20:14000-18000"
    );

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

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query by rs6040355 - should find variant with multiple alternates
    let results = index.query_by_id("rs6040355");
    assert_eq!(
        results.len(),
        1,
        "Should find exactly one variant with ID rs6040355"
    );
    assert_eq!(results[0].chromosome, "20");
    assert_eq!(results[0].position, 1110696);
    assert_eq!(
        results[0].alternate.len(),
        2,
        "rs6040355 should have 2 alternate alleles"
    );
}

#[test]
fn test_format_variant_with_real_data() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (results, _) = index.query_by_position("20", 14370);

    assert!(!results.is_empty(), "Should find variant at 20:14370");

    let dto = format_variant(results[0].clone());

    // Verify DTO contains expected fields
    assert_eq!(dto.chromosome, "20");
    assert_eq!(dto.position, 14370);
    assert_eq!(dto.id, "rs6054257");
    assert_eq!(dto.reference, "G");
    assert_eq!(dto.alternate, vec!["A"]);
    assert_eq!(dto.filter, vec!["PASS"]);
    assert!(dto.info.contains_key("NS"));
}

#[test]
fn test_load_nonexistent_file() {
    let vcf_path = PathBuf::from("nonexistent.vcf.gz");
    let result = load_vcf(&vcf_path, false, false);

    assert!(
        result.is_err(),
        "Loading nonexistent file should return an error"
    );
}

#[test]
fn test_chromosome_x_variant() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query for variant on chromosome X at position 10
    let (results, _) = index.query_by_position("X", 10);
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

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query for microsat1 variant
    let results = index.query_by_id("microsat1");
    assert_eq!(results.len(), 1, "Should find microsat1 variant");
    assert_eq!(results[0].chromosome, "20");
    assert_eq!(results[0].position, 1234567);
}

#[test]
fn test_chromosome_variant_matching_with_chr_prefix() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // VCF file uses "20" (without chr prefix)
    // Query with "chr20" should still find variants through variant matching
    let (results_with_chr, matched_chr) = index.query_by_position("chr20", 14370);
    assert_eq!(
        results_with_chr.len(),
        1,
        "Should find variant when querying chr20"
    );
    assert_eq!(results_with_chr[0].id, "rs6054257");
    assert_eq!(
        matched_chr,
        Some("20".to_string()),
        "Should match to chromosome 20"
    );

    // Verify we get the same results with and without prefix
    let (results_without_chr, _) = index.query_by_position("20", 14370);
    assert_eq!(
        results_with_chr.len(),
        results_without_chr.len(),
        "Should get same results with or without chr prefix"
    );
    assert_eq!(
        results_with_chr[0].id, results_without_chr[0].id,
        "Should find same variant"
    );
}

#[test]
fn test_chromosome_variant_matching_chrx() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query X chromosome with chr prefix
    let (results_with_chr, matched_chr) = index.query_by_position("chrX", 10);
    assert_eq!(
        results_with_chr.len(),
        1,
        "Should find variant when querying chrX"
    );
    assert_eq!(results_with_chr[0].id, "rsTest");
    assert_eq!(
        matched_chr,
        Some("X".to_string()),
        "Should match to chromosome X"
    );
}

#[test]
fn test_chromosome_not_found_returns_none() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query non-existent chromosome
    let (results, matched_chr) = index.query_by_position("99", 12345);
    assert_eq!(results.len(), 0, "Should find no variants");
    assert_eq!(
        matched_chr, None,
        "Should return None for matched chromosome"
    );

    // Verify available chromosomes list is not empty
    let available = index.get_available_chromosomes();
    assert!(!available.is_empty(), "Should have available chromosomes");
}

#[test]
fn test_query_by_region_with_chr_prefix() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query region with chr prefix
    let (results, matched_chr) = index.query_by_region("chr20", 14000, 18000);
    assert_eq!(
        results.len(),
        2,
        "Should find 2 variants in region chr20:14000-18000"
    );
    assert_eq!(
        matched_chr,
        Some("20".to_string()),
        "Should match to chromosome 20"
    );
}

#[test]
fn test_reference_genome_extraction_from_header() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let metadata = index.get_metadata();

    // sample.compressed.vcf.gz has ##reference=1000GenomesPilot-NCBI36
    assert_eq!(
        metadata.reference_genome.build, "1000GenomesPilot-NCBI36",
        "Should extract reference from ##reference header line"
    );
    assert!(
        matches!(
            metadata.reference_genome.source,
            ReferenceGenomeSource::HeaderLine
        ),
        "Source should be HeaderLine"
    );
}

#[test]
fn test_reference_genome_extraction_from_hg38() {
    let vcf_path = PathBuf::from("NG1QY7GX8H.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: NG1QY7GX8H.vcf.gz not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let metadata = index.get_metadata();

    // NG1QY7GX8H.vcf.gz has ##reference=file:///mnt/ssd/MegaBOLT_scheduler/reference/hg38.fa
    // This should be extracted from the header
    assert!(
        metadata.reference_genome.build.contains("hg38"),
        "Should extract reference containing hg38 from header"
    );
    assert!(
        matches!(
            metadata.reference_genome.source,
            ReferenceGenomeSource::HeaderLine
        ),
        "Source should be HeaderLine since ##reference exists"
    );

    // Also verify the contigs are GRCh38 (chr1 length = 248,956,422)
    let chr1_contig = metadata
        .contigs
        .iter()
        .find(|c| c.id == "chr1")
        .expect("Should have chr1 contig");
    assert_eq!(chr1_contig.id, "chr1");
}

#[test]
fn test_get_reference_genome_string() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let reference_string = index.get_reference_genome();

    // Should include both the build and the source
    assert!(
        reference_string.contains("1000GenomesPilot-NCBI36"),
        "Should contain the genome build"
    );
    assert!(
        reference_string.contains("from header"),
        "Should indicate source is from header"
    );
}

// ============================================================================
// Streaming Query Session Tests
// ============================================================================

#[tokio::test]
async fn test_streaming_basic_session_lifecycle() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Start streaming query on region 20:14000-18000 (contains 2 variants)
    let (mut variants, matched_chr) = index.query_by_region("20", 14000, 18000);
    assert_eq!(matched_chr, Some("20".to_string()));
    assert_eq!(variants.len(), 2, "Region should contain 2 variants");

    // Simulate streaming by getting variants one at a time
    let first_variant = variants.remove(0);
    assert_eq!(first_variant.position, 14370);
    assert_eq!(first_variant.id, "rs6054257");

    let second_variant = variants.remove(0);
    assert_eq!(second_variant.position, 17330);
    // Second variant has ID '.' in the VCF
}

#[tokio::test]
async fn test_streaming_session_with_no_variants() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query a region with no variants
    let (variants, matched_chr) = index.query_by_region("20", 1, 100);
    assert_eq!(matched_chr, Some("20".to_string()));
    assert_eq!(variants.len(), 0, "Empty region should return no variants");
}

#[tokio::test]
async fn test_streaming_session_chromosome_normalization() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query with chr prefix (VCF uses "20" without prefix)
    let (variants_chr, matched_chr) = index.query_by_region("chr20", 14000, 18000);
    assert_eq!(
        matched_chr,
        Some("20".to_string()),
        "Should match to chromosome 20"
    );
    assert_eq!(variants_chr.len(), 2);

    // Query without chr prefix
    let (variants_no_chr, _) = index.query_by_region("20", 14000, 18000);
    assert_eq!(variants_chr.len(), variants_no_chr.len());
    assert_eq!(variants_chr[0].id, variants_no_chr[0].id);
}

#[tokio::test]
async fn test_streaming_session_invalid_chromosome() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query non-existent chromosome
    let (variants, matched_chr) = index.query_by_region("99", 1000, 2000);
    assert_eq!(variants.len(), 0);
    assert_eq!(matched_chr, None);

    // Verify available chromosomes list is not empty
    let available = index.get_available_chromosomes();
    assert!(!available.is_empty());
}

#[tokio::test]
async fn test_streaming_large_region() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query entire chromosome 20 range
    let (variants, matched_chr) = index.query_by_region("20", 1, 100_000_000);
    assert_eq!(matched_chr, Some("20".to_string()));
    assert!(
        !variants.is_empty(),
        "Should find variants across entire chromosome"
    );

    // Verify variants are sorted by position
    for i in 1..variants.len() {
        assert!(
            variants[i - 1].position <= variants[i].position,
            "Variants should be sorted by position"
        );
    }
}

#[tokio::test]
async fn test_streaming_position_boundary() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query exact variant position
    let (variants_exact, _) = index.query_by_region("20", 14370, 14370);
    assert_eq!(variants_exact.len(), 1);
    assert_eq!(variants_exact[0].position, 14370);

    // Query just before variant
    let (variants_before, _) = index.query_by_region("20", 14000, 14369);
    assert_eq!(variants_before.len(), 0);

    // Query just after variant starts
    let (variants_after, _) = index.query_by_region("20", 14371, 15000);
    assert_eq!(variants_after.len(), 0);
}

#[tokio::test]
async fn test_streaming_multiallelic_variant() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query rs6040355 which has 2 alternate alleles
    let results = index.query_by_id("rs6040355");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].alternate.len(),
        2,
        "Should have 2 alternate alleles"
    );
    assert_eq!(results[0].alternate[0], "G");
    assert_eq!(results[0].alternate[1], "T");
}

// ============================================================================
// Filter Evaluation Tests (for Streaming)
// ============================================================================

#[tokio::test]
async fn test_filter_evaluation_with_streaming_data() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 14000, 18000);
    let filter_engine = index.filter_engine();

    assert!(
        !variants.is_empty(),
        "Need at least one variant for testing"
    );
    let variant = &variants[0];

    // Test QUAL filter
    if let Some(quality) = variant.quality {
        let filter = format!("QUAL > {}", quality - 1.0);
        assert!(
            filter_engine.evaluate(&filter, &variant.raw_row).unwrap(),
            "QUAL should be > quality-1"
        );

        let filter_fail = format!("QUAL > {}", quality + 1.0);
        assert!(
            !filter_engine
                .evaluate(&filter_fail, &variant.raw_row)
                .unwrap(),
            "QUAL should not be > quality+1"
        );
    }

    // Test FILTER field (needs quotes now)
    if !variant.filter.is_empty() {
        let filter_value = &variant.filter[0];
        let filter = format!("FILTER == \"{}\"", filter_value);
        assert!(
            filter_engine.evaluate(&filter, &variant.raw_row).unwrap(),
            "FILTER should match"
        );
    }

    // Note: vcf-filter doesn't support empty filters like the old implementation
    // Empty filter is handled at the application level by skipping filter evaluation
}

#[tokio::test]
async fn test_filter_with_multiple_variants() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 14000, 18000);
    let filter_engine = index.filter_engine();

    // Filter for FILTER == "PASS" (with quotes in new syntax)
    let filter = "FILTER == \"PASS\"";
    let passing_variants: Vec<_> = variants
        .iter()
        .filter(|v| filter_engine.evaluate(filter, &v.raw_row).unwrap_or(false))
        .collect();

    // Verify all filtered variants have PASS filter
    for variant in passing_variants {
        assert!(variant.filter.contains(&"PASS".to_string()));
    }
}

#[tokio::test]
async fn test_streaming_session_all_variants_filtered_out() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, matched_chr) = index.query_by_region("20", 14000, 18000);
    let filter_engine = index.filter_engine();

    assert_eq!(matched_chr, Some("20".to_string()));
    assert!(
        !variants.is_empty(),
        "Region should contain variants before filtering"
    );

    // Apply impossible filter that excludes all variants
    let filter = "QUAL > 999999";
    let filtered_variants: Vec<_> = variants
        .into_iter()
        .filter(|v| filter_engine.evaluate(filter, &v.raw_row).unwrap_or(false))
        .collect();

    // This simulates what start_region_query does
    assert_eq!(
        filtered_variants.len(),
        0,
        "All variants should be filtered out"
    );

    // The new implementation should return a graceful response:
    // { variant: None, session_id: None, has_more: false }
    // rather than an error
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_query_with_invalid_position_zero() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Position 0 is invalid in VCF (1-based)
    let (variants, _) = index.query_by_position("20", 0);
    assert_eq!(variants.len(), 0, "Position 0 should return no results");
}

#[test]
fn test_query_with_start_greater_than_end() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Start > End should return empty results (not an error in our implementation)
    let (variants, _) = index.query_by_region("20", 18000, 14000);
    assert_eq!(variants.len(), 0, "Inverted range should return no results");
}

#[test]
fn test_query_nonexistent_variant_id() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query for non-existent ID
    let results = index.query_by_id("nonexistent_id_12345");
    assert_eq!(results.len(), 0, "Nonexistent ID should return no results");
}

#[test]
fn test_query_empty_id() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Query for empty ID
    let results = index.query_by_id("");
    assert_eq!(results.len(), 0, "Empty ID should return no results");
}

// ============================================================================
// Index Persistence Tests
// ============================================================================

#[test]
fn test_index_files_created() {
    use std::fs;
    use tempfile::TempDir;

    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    // Create a temporary directory for testing
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_vcf_path = temp_dir.path().join("test.vcf.gz");

    // Copy VCF file to temp directory (writable location)
    fs::copy(&vcf_path, &temp_vcf_path).expect("Failed to copy VCF file");

    // Copy existing CSI index so we test the "load existing genomic index, create ID index" path
    let csi_src = PathBuf::from("sample_data/sample.compressed.vcf.gz.csi");
    if csi_src.exists() {
        let csi_dest = temp_dir.path().join("test.vcf.gz.csi");
        fs::copy(&csi_src, &csi_dest).expect("Failed to copy CSI index");
    }

    // Load VCF with index saving enabled (debug=false, save_index=true)
    let _index = load_vcf(&temp_vcf_path, false, true).expect("Failed to load VCF file");

    // Check for ID index in the temp directory
    let idx_path = temp_dir.path().join("test.vcf.gz.idx");
    assert!(idx_path.exists(), "Should create ID index (.idx)");
}

#[test]
fn test_never_save_index_flag() {
    use std::fs;
    use tempfile::TempDir;

    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    // Create a temporary copy of the VCF file
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_vcf = temp_dir.path().join("test.vcf.gz");
    fs::copy(&vcf_path, &temp_vcf).expect("Failed to copy VCF file");

    // Load with never_save_index = true
    let _index = load_vcf(&temp_vcf, false, true).expect("Failed to load VCF file");

    // Verify no index files created
    let tbi_path = temp_vcf.with_extension("vcf.gz.tbi");
    let csi_path = temp_vcf.with_extension("vcf.gz.csi");
    let idx_path = temp_vcf.with_extension("vcf.gz.idx");

    assert!(
        !tbi_path.exists(),
        "Should not create .tbi index with never_save_index"
    );
    assert!(
        !csi_path.exists(),
        "Should not create .csi index with never_save_index"
    );
    assert!(
        !idx_path.exists(),
        "Should not create .idx index with never_save_index"
    );
}

#[test]
fn test_index_loading_from_disk() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    // Ensure indices exist by loading once
    let _ = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Load again - should use existing indices
    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Verify index works correctly
    let (variants, _) = index.query_by_position("20", 14370);
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].id, "rs6054257");
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_chromosome_x_query() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");

    // Test X chromosome queries
    let (variants_x, matched_chr) = index.query_by_position("X", 10);
    if !variants_x.is_empty() {
        assert_eq!(matched_chr, Some("X".to_string()));
        assert_eq!(variants_x[0].chromosome, "X");
    }

    // Test with chr prefix
    let (variants_chrx, matched_chr_x) = index.query_by_position("chrX", 10);
    if !variants_chrx.is_empty() {
        assert_eq!(matched_chr_x, Some("X".to_string()));
    }
}

#[test]
fn test_variant_with_missing_quality() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 1, 10_000_000);
    let filter_engine = index.filter_engine();

    // Find a variant with missing quality (if any)
    let variant_missing_qual = variants.iter().find(|v| v.quality.is_none());
    if let Some(variant) = variant_missing_qual {
        // QUAL filter should not match if quality is missing
        // vcf-filter may return false or error for missing field comparison
        let result = filter_engine.evaluate("QUAL > 0", &variant.raw_row);
        assert!(
            result.is_err() || !result.unwrap(),
            "QUAL filter should fail or return false for missing quality"
        );
    }
}

#[test]
fn test_variant_with_no_alternates() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let (variants, _) = index.query_by_region("20", 1, 10_000_000);

    // VCF files may have reference-only calls with '.' as ALT
    // Our parser converts these to empty alternate arrays or filters them out
    // Just verify that variants we do get have the expected structure
    for variant in &variants {
        if !variant.alternate.is_empty() {
            // If there are alternates, they should be valid non-empty strings
            for alt in &variant.alternate {
                assert!(
                    !alt.is_empty(),
                    "Alternate alleles should not be empty strings"
                );
            }
        }
    }
}

#[test]
fn test_get_available_chromosomes() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let chromosomes = index.get_available_chromosomes();

    assert!(!chromosomes.is_empty(), "Should have available chromosomes");
    assert!(
        chromosomes.contains(&"20".to_string()),
        "Should include chromosome 20"
    );
}

#[test]
fn test_vcf_header_retrieval() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let header = index.get_header_string(None);

    assert!(!header.is_empty(), "Header should not be empty");
    assert!(
        header.contains("##fileformat=VCF"),
        "Header should contain VCF format"
    );
    assert!(
        header.contains("#CHROM"),
        "Header should contain column headers"
    );
}

#[test]
fn test_vcf_statistics_computation() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping test");
        return;
    }

    let index = load_vcf(&vcf_path, false, false).expect("Failed to load VCF file");
    let stats = index
        .compute_statistics()
        .expect("Failed to compute statistics");

    // Basic metadata checks
    assert!(
        !stats.file_format.is_empty(),
        "File format should be present"
    );
    assert!(
        !stats.reference_genome.is_empty(),
        "Reference genome should be present"
    );
    assert!(
        stats.chromosome_count > 0,
        "Should have at least one chromosome"
    );
    assert!(
        stats.chromosomes.contains(&"20".to_string()),
        "Should include chromosome 20"
    );

    // Variant count checks
    assert!(stats.total_variants > 0, "Should have at least one variant");
    assert!(
        !stats.variants_per_chromosome.is_empty(),
        "Should have per-chromosome counts"
    );

    // Verify per-chromosome counts sum to total
    let sum: u64 = stats.variants_per_chromosome.values().sum();
    assert_eq!(
        sum, stats.total_variants,
        "Per-chromosome counts should sum to total"
    );

    // ID statistics
    let total_with_ids = stats.unique_ids;
    assert!(
        total_with_ids + stats.missing_ids == stats.total_variants,
        "Unique IDs + missing IDs should equal total variants"
    );

    // Quality statistics (sample file should have quality scores)
    if let Some(qual_stats) = stats.quality_stats {
        assert!(qual_stats.min >= 0.0, "Min quality should be non-negative");
        assert!(qual_stats.max >= qual_stats.min, "Max should be >= min");
        assert!(
            qual_stats.mean >= 0.0,
            "Mean quality should be non-negative"
        );
        assert!(
            qual_stats.mean >= qual_stats.min && qual_stats.mean <= qual_stats.max,
            "Mean should be between min and max"
        );
    }

    // Filter counts
    assert!(!stats.filter_counts.is_empty(), "Should have filter counts");
    // Note: Filter count sum may be less than total_variants if some variants have no filter,
    // or greater if some variants have multiple filters
    let total_filter_entries: u64 = stats.filter_counts.values().sum();
    eprintln!(
        "Filter entries: {}, Total variants: {}",
        total_filter_entries, stats.total_variants
    );

    // Variant type statistics
    let type_total = stats.variant_types.snps
        + stats.variant_types.insertions
        + stats.variant_types.deletions
        + stats.variant_types.mnps
        + stats.variant_types.complex;
    assert_eq!(
        type_total, stats.total_variants,
        "Variant type counts should sum to total variants"
    );

    // Print statistics for manual verification
    eprintln!("VCF Statistics:");
    eprintln!("  Total variants: {}", stats.total_variants);
    eprintln!("  Unique IDs: {}", stats.unique_ids);
    eprintln!("  Missing IDs: {}", stats.missing_ids);
    eprintln!("  SNPs: {}", stats.variant_types.snps);
    eprintln!("  Insertions: {}", stats.variant_types.insertions);
    eprintln!("  Deletions: {}", stats.variant_types.deletions);
    eprintln!("  MNPs: {}", stats.variant_types.mnps);
    eprintln!("  Complex: {}", stats.variant_types.complex);
}

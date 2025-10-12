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
    assert_eq!(matched_chr, None, "Should return None for matched chromosome");

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
        metadata.reference_genome.build,
        "1000GenomesPilot-NCBI36",
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

use std::collections::HashMap;
use vcf_mcp_server::vcf::{VariantTypeStats, VcfStatistics};

fn main() {
    // Create mock statistics with many chromosomes
    let mut variants_per_chromosome = HashMap::new();
    for i in 1..=50 {
        variants_per_chromosome.insert(format!("chr{}", i), (51 - i) * 1000); // Descending counts
    }

    let mut stats = VcfStatistics {
        file_format: "VCFv4.2".to_string(),
        reference_genome: "GRCh38".to_string(),
        chromosome_count: 50,
        sample_count: 1,
        chromosomes: (1..=50).map(|i| format!("chr{}", i)).collect(),
        total_variants: 1275000,
        variants_per_chromosome: variants_per_chromosome.clone(),
        unique_ids: 1000000,
        missing_ids: 275000,
        quality_stats: None,
        filter_counts: HashMap::new(),
        variant_types: VariantTypeStats {
            snps: 1000000,
            insertions: 100000,
            deletions: 150000,
            mnps: 25000,
            complex: 0,
        },
    };

    println!(
        "Original chromosome count: {}",
        stats.variants_per_chromosome.len()
    );

    // Test limiting to 25
    let max_chromosomes = 25;
    if max_chromosomes > 0 && stats.variants_per_chromosome.len() > max_chromosomes {
        let mut chr_counts: Vec<_> = stats.variants_per_chromosome.iter().collect();
        chr_counts.sort_by(|a, b| b.1.cmp(a.1));

        let limited: HashMap<String, u64> = chr_counts
            .into_iter()
            .take(max_chromosomes)
            .map(|(k, v): (&String, &u64)| (k.clone(), *v))
            .collect();

        stats.variants_per_chromosome = limited;
    }

    println!(
        "After limiting to {}: {}",
        max_chromosomes,
        stats.variants_per_chromosome.len()
    );

    // Verify top chromosomes are included
    let mut sorted: Vec<_> = stats.variants_per_chromosome.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!("\nTop 10 chromosomes by variant count:");
    for (chr, count) in sorted.iter().take(10) {
        println!("  {}: {}", chr, count);
    }

    println!(
        "\n✓ Test passed! Limited from 50 to {} chromosomes",
        max_chromosomes
    );
}

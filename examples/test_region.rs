use std::path::PathBuf;
use vcf_mcp_server::vcf::load_vcf;

fn main() {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    let index = load_vcf(&vcf_path, false, true).expect("Failed to load VCF file");

    println!("Test 1: Small region chr20:14000-18000");
    let (variants, matched): (Vec<_>, Option<String>) =
        index.query_by_region("chr20", 14000, 18000);
    println!(
        "  Found {} variants, matched: {:?}",
        variants.len(),
        matched
    );

    println!("\nTest 2: Large region 20:1-1000000000");
    let (variants, matched): (Vec<_>, Option<String>) =
        index.query_by_region("20", 1, 1_000_000_000);
    println!(
        "  Found {} variants, matched: {:?}",
        variants.len(),
        matched
    );

    println!("\nTest 3: Tabix max region 20:1-536870912");
    let (variants, matched): (Vec<_>, Option<String>) = index.query_by_region("20", 1, 536_870_912);
    println!(
        "  Found {} variants, matched: {:?}",
        variants.len(),
        matched
    );

    println!("\nTest 4: Exact region 20:14370-17330");
    let (variants, matched): (Vec<_>, Option<String>) = index.query_by_region("20", 14370, 17330);
    println!(
        "  Found {} variants, matched: {:?}",
        variants.len(),
        matched
    );
}

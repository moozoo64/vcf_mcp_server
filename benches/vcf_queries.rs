use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use vcf_mcp_server::vcf::load_vcf;

fn setup_vcf_index() -> vcf_mcp_server::vcf::VcfIndex {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
    load_vcf(&vcf_path, false, false).expect("Failed to load VCF file")
}

fn benchmark_query_by_position(c: &mut Criterion) {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    // Skip benchmark if sample file doesn't exist
    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping benchmark");
        return;
    }

    let index = setup_vcf_index();

    c.bench_function("query_by_position", |b| {
        b.iter(|| {
            let (results, _) = index.query_by_position(black_box("20"), black_box(14370));
            black_box(results);
        })
    });
}

fn benchmark_query_by_region(c: &mut Criterion) {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping benchmark");
        return;
    }

    let index = setup_vcf_index();

    c.bench_function("query_by_region", |b| {
        b.iter(|| {
            let (results, _) = index.query_by_region(black_box("20"), black_box(14000), black_box(18000));
            black_box(results);
        })
    });
}

fn benchmark_query_by_id(c: &mut Criterion) {
    let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");

    if !vcf_path.exists() {
        eprintln!("Warning: Sample VCF file not found, skipping benchmark");
        return;
    }

    let index = setup_vcf_index();

    c.bench_function("query_by_id", |b| {
        b.iter(|| {
            let results = index.query_by_id(black_box("rs6054257"));
            black_box(results);
        })
    });
}

criterion_group!(benches, benchmark_query_by_position, benchmark_query_by_region, benchmark_query_by_id);
criterion_main!(benches);

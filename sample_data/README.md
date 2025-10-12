# Test Data Attribution

This directory contains sample VCF files used for testing and demonstration purposes.

## VCFlib Sample Files

**Files:**
- `sample.compressed.vcf.gz`
- `sample.compressed.vcf.gz.tbi`

**Source:** https://github.com/vcflib/vcflib/tree/master/samples

**License:** MIT License

VCFlib is a C++ library for parsing and manipulating VCF files. These sample files are provided as part of the VCFlib project for testing purposes.

## 1000 Genomes Project Data

**File:**
- `1kGP_high_coverage_Illumina.chr1.filtered.SNV_INDEL_SV_phased_panel-head-1000.vcf.gz`

**Source:** 1000 Genomes Project / International Genome Sample Resource (IGSR)

**Citation:**
> A global reference for human genetic variation, The 1000 Genomes Project Consortium, *Nature* 526, 68-74 (01 October 2015) doi:10.1038/nature15393

**License:** The 1000 Genomes Project data is available under a Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported license.

This file contains a subset (first 1000 lines) of high-coverage Illumina sequencing data from chromosome 1, filtered for SNVs, INDELs, and structural variants with phasing information.

## 1001 Genomes Arabidopsis Data

**Files:**
- `arabidopsis_thaliana_chr1_subset.vcf.gz`
- `arabidopsis_thaliana_chr1_subset.vcf.gz.tbi`
- `download_arabidopsis_sample.sh`

**Source:** 1001 Genomes Project for Arabidopsis thaliana (https://1001genomes.org/)

**Dataset:** GMI-MPI v3.1 release - SNPs and short indels only

**Citation:**
> 1,135 Genomes Reveal the Global Pattern of Polymorphism in Arabidopsis thaliana, The 1001 Genomes Consortium, *Cell* 166, 481–491 (July 28, 2016) doi:10.1016/j.cell.2016.05.063

**License:** The 1001 Genomes Project data is available under open access terms. Please see https://1001genomes.org/ for current licensing information.

**Content:** This file contains ~5,000 variants from the first 100kb of Arabidopsis thaliana chromosome 1 from the 1,135 accession study. The data represents natural genetic variation across 1,135 strains with genotype information for each sample. The reference genome is TAIR10.

**Genome Information:**
- Organism: *Arabidopsis thaliana* (thale cress)
- Chromosomes: 1, 2, 3, 4, 5
- Genome size: ~135 Mb
- Reference: TAIR10

**Reproduction:** Run `./download_arabidopsis_sample.sh` to regenerate this dataset from the source.

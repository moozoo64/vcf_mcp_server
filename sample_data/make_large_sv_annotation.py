#!/usr/bin/env python3
"""Generate sample_data/large-sv-annotation.vcf.gz (+ .csi) for response-size tests.

Models a Manta structural-variant call whose ANN field lists one entry per
overlapped gene: a deletion spanning chr1:789477-224012315 (FILTER SV_HARD)
with 22,813 ANN entries (~2.5 MB of JSON once parsed), plus a few ordinary
small records inside it. Querying any position under the deletion returns
that record, which is what produced multi-megabyte tool responses.

Requires bgzip and tabix (htslib). Run from the repository root:

    python3 sample_data/make_large_sv_annotation.py
"""

import os
import subprocess

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "large-sv-annotation.vcf")
ANN_ENTRIES = 22813

header = """##fileformat=VCFv4.2
##reference=GRCh38
##contig=<ID=chr1,length=248956422>
##INFO=<ID=END,Number=1,Type=Integer,Description="End position of the variant">
##INFO=<ID=SVTYPE,Number=1,Type=String,Description="Type of structural variant">
##INFO=<ID=SVLEN,Number=.,Type=Integer,Description="Difference in length between REF and ALT alleles">
##INFO=<ID=ANN,Number=.,Type=String,Description="Functional annotations: 'Allele | Annotation | Annotation_Impact | Gene_Name | Gene_ID | Feature_Type | Feature_ID | Transcript_BioType'">
##FILTER=<ID=PASS,Description="All filters passed">
##FILTER=<ID=SV_HARD,Description="Failed hard structural-variant filters">
##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tSAMPLE1
"""

ann = ",".join(
    f"<DEL>|feature_ablation|HIGH|GENE{i}|ENSG{i:011d}|transcript|ENST{i:011d}|protein_coding"
    for i in range(1, ANN_ENTRIES + 1)
)

rows = [
    ["chr1", "789477", "MantaDEL:6:7:8:0:0:0", "N", "<DEL>", ".", "SV_HARD",
     f"END=224012315;SVTYPE=DEL;SVLEN=-223222838;ANN={ann}", "GT", "0/1"],
    ["chr1", "1207339", "rsSmall1", "A", "G", "50", "PASS",
     "ANN=G|missense_variant|MODERATE|GENE42|ENSG00000000042|transcript|ENST00000000042|protein_coding",
     "GT", "0/1"],
    ["chr1", "1207339", "rsSmall2", "A", "T", "40", "PASS", ".", "GT", "0/1"],
    ["chr1", "1300000", "rsSmall3", "C", "T", "60", "PASS", ".", "GT", "1/1"],
]

with open(OUT, "w") as f:
    f.write(header)
    for row in rows:
        f.write("\t".join(row) + "\n")

subprocess.run(["bgzip", "-f", OUT], check=True)
subprocess.run(["tabix", "-f", "-C", "-p", "vcf", OUT + ".gz"], check=True)
print(f"wrote {OUT}.gz and {OUT}.gz.csi")

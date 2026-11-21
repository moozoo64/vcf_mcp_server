//! Keeps variant-returning tool responses under a byte budget.
//!
//! A single VCF record can be megabytes of JSON: a structural variant whose
//! `ANN` field lists every gene it overlaps, or a record with thousands of
//! samples. Responses that large break MCP clients, so each tool that returns
//! variants passes them through [`fit_variants`]:
//!
//! - Records are added in order until the budget is used up. Remaining records
//!   are left out and the caller reports `truncated: true`.
//! - A record that is larger than the whole budget on its own is shrunk
//!   instead of dropped (see [`shrink_record`]), so it is still reported and
//!   paging can always move past it.

use crate::vcf::Variant;
use serde_json::{Value, json};

/// Default value for `--max-response-bytes`.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 100 * 1024;

/// Bytes kept back from the budget for the response envelope (status, query
/// echo, hint, ...).
const ENVELOPE_RESERVE: usize = 4 * 1024;

/// Smallest budget given to records, however low `--max-response-bytes` is set.
const MIN_VARIANT_BUDGET: usize = 1024;

/// INFO values up to this size are kept in the first shrinking pass, so small
/// fields such as SVTYPE, END and SVLEN survive.
const SMALL_INFO_VALUE_BYTES: usize = 256;

/// REF/ALT alleles longer than this are shortened when a record is shrunk.
const MAX_ALLELE_CHARS: usize = 100;

/// Result of fitting variants into a byte budget.
pub struct FittedVariants {
    /// Serialized records. Always a prefix of the input, in input order.
    pub items: Vec<Value>,
    /// True when some input records were left out to stay under the budget.
    pub truncated: bool,
    /// Number of returned records that were shrunk (`record_truncated: true`).
    pub records_shrunk: usize,
}

/// Bytes available for records in a response limited to `max_response_bytes`,
/// after reserving room for the envelope and `extra_envelope_bytes` (e.g. an
/// echoed list of requested IDs).
pub fn variant_budget(max_response_bytes: usize, extra_envelope_bytes: usize) -> usize {
    max_response_bytes
        .saturating_sub(ENVELOPE_RESERVE + extra_envelope_bytes)
        .max(MIN_VARIANT_BUDGET)
}

/// Serialize `variants` in order, keeping the total under `budget` bytes.
///
/// At least one record is always returned when the input is not empty, so a
/// caller paging through results always makes progress.
pub fn fit_variants(variants: &[Variant], budget: usize) -> FittedVariants {
    let mut items = Vec::new();
    let mut records_shrunk = 0;
    let mut used = 2; // the enclosing `[` and `]`

    for variant in variants {
        let mut record = serde_json::to_value(variant).unwrap_or(Value::Null);
        let mut size = json_len(&record);
        let shrunk = size > budget;
        if shrunk {
            shrink_record(&mut record, budget);
            size = json_len(&record);
        }

        let separator = usize::from(!items.is_empty());
        if !items.is_empty() && used + separator + size > budget {
            return FittedVariants {
                items,
                truncated: true,
                records_shrunk,
            };
        }

        used += separator + size;
        records_shrunk += usize::from(shrunk);
        items.push(record);
    }

    FittedVariants {
        items,
        truncated: false,
        records_shrunk,
    }
}

/// Shrink a serialized variant so it fits in `limit` bytes where possible.
///
/// Steps, each applied only while the record is still too large:
/// 1. Omit the largest parts first: INFO values over 256 bytes and the whole
///    `samples` map are candidates, largest first. An omitted INFO value is
///    replaced by `{"omitted": true, "entries": N, "bytes": N}`; omitted
///    samples become `samples: {}` plus `samples_omitted: <count>`.
/// 2. Shorten REF/ALT alleles longer than 100 characters.
/// 3. Omit every remaining INFO value.
///
/// The record is marked `record_truncated: true`. If it is still over the
/// limit after all steps it is returned as it is.
pub fn shrink_record(record: &mut Value, limit: usize) {
    let Some(obj) = record.as_object_mut() else {
        return;
    };
    obj.insert("record_truncated".into(), Value::Bool(true));

    // Step 1: omit the largest parts first.
    enum Part {
        Info(String),
        Samples,
    }
    let mut parts: Vec<(Part, usize)> = Vec::new();
    if let Some(Value::Object(info)) = obj.get("info") {
        for (key, value) in info {
            let size = json_len(value);
            if size > SMALL_INFO_VALUE_BYTES {
                parts.push((Part::Info(key.clone()), size));
            }
        }
    }
    if let Some(Value::Object(samples)) = obj.get("samples")
        && !samples.is_empty()
    {
        parts.push((Part::Samples, json_len(&obj["samples"])));
    }
    parts.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

    for (part, _) in parts {
        if json_len(record) <= limit {
            return;
        }
        match part {
            Part::Info(key) => {
                let marker = omitted_marker(&record["info"][&key]);
                record["info"][&key] = marker;
            }
            Part::Samples => {
                let count = record["samples"].as_object().map_or(0, |s| s.len());
                record["samples"] = json!({});
                record["samples_omitted"] = json!(count);
            }
        }
    }

    // Step 2: shorten very long alleles (e.g. explicit insertion sequences).
    if json_len(record) > limit {
        if let Some(reference) = record["reference"].as_str() {
            record["reference"] = Value::String(shorten_allele(reference));
        }
        if let Some(alternates) = record["alternate"].as_array_mut() {
            for allele in alternates.iter_mut() {
                if let Some(text) = allele.as_str() {
                    *allele = Value::String(shorten_allele(text));
                }
            }
        }
    }

    // Step 3: omit every remaining INFO value.
    if json_len(record) > limit
        && let Some(info) = record["info"].as_object_mut()
    {
        for value in info.values_mut() {
            if value.get("omitted").is_none() {
                *value = omitted_marker(value);
            }
        }
    }
}

fn json_len(value: &Value) -> usize {
    serde_json::to_string(value).map_or(0, |s| s.len())
}

fn omitted_marker(value: &Value) -> Value {
    let entries = match value {
        Value::Array(items) => items.len(),
        Value::String(text) => text.split(',').count(),
        _ => 1,
    };
    json!({ "omitted": true, "entries": entries, "bytes": json_len(value) })
}

fn shorten_allele(allele: &str) -> String {
    let length = allele.chars().count();
    if length <= MAX_ALLELE_CHARS {
        return allele.to_string();
    }
    let prefix: String = allele.chars().take(MAX_ALLELE_CHARS).collect();
    format!("{prefix}...({length} bp)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn variant(position: u64, id: &str) -> Variant {
        Variant {
            chromosome: "chr1".into(),
            position,
            id: id.into(),
            reference: "A".into(),
            alternate: vec!["G".into()],
            quality: Some(50.0),
            filter: vec!["PASS".into()],
            info: HashMap::new(),
            samples: HashMap::new(),
            raw_row: String::new(),
        }
    }

    fn sv_with_ann(entries: usize) -> Variant {
        let mut v = variant(789477, "MantaDEL:6:7:8:0:0:0");
        v.reference = "N".into();
        v.alternate = vec!["<DEL>".into()];
        v.filter = vec!["SV_HARD".into()];
        let ann: Vec<Value> = (0..entries)
            .map(|i| json!(format!("<DEL>|feature_ablation|HIGH|GENE{i}|ENSG{i:011}")))
            .collect();
        v.info.insert("ANN".into(), Value::Array(ann));
        v.info.insert("SVTYPE".into(), json!("DEL"));
        v.info.insert("END".into(), json!(224012315));
        v
    }

    fn total_len(items: &[Value]) -> usize {
        json_len(&Value::Array(items.to_vec()))
    }

    #[test]
    fn small_records_all_fit() {
        let variants: Vec<_> = (0..5).map(|i| variant(100 + i, "rs")).collect();
        let fitted = fit_variants(&variants, 10_000);
        assert_eq!(fitted.items.len(), 5);
        assert!(!fitted.truncated);
        assert_eq!(fitted.records_shrunk, 0);
    }

    #[test]
    fn stops_at_budget_and_reports_truncation() {
        let variants: Vec<_> = (0..50).map(|i| variant(100 + i, "rs")).collect();
        let one = json_len(&serde_json::to_value(&variants[0]).unwrap());
        let budget = one * 3 + 10;
        let fitted = fit_variants(&variants, budget);
        assert_eq!(fitted.items.len(), 3, "should keep exactly what fits");
        assert!(fitted.truncated);
        assert!(total_len(&fitted.items) <= budget);
        // Returned records are the input prefix, in order.
        for (i, item) in fitted.items.iter().enumerate() {
            assert_eq!(item["position"], 100 + i as u64);
        }
    }

    #[test]
    fn always_returns_at_least_one_record() {
        let variants = vec![sv_with_ann(1000), variant(2, "rs2")];
        let fitted = fit_variants(&variants, 64);
        assert_eq!(fitted.items.len(), 1);
        assert!(fitted.truncated);
        assert_eq!(fitted.records_shrunk, 1);
    }

    #[test]
    fn oversized_ann_is_replaced_by_marker_and_small_info_kept() {
        let variants = vec![sv_with_ann(22813), variant(1207339, "rsSmall1")];
        let budget = variant_budget(DEFAULT_MAX_RESPONSE_BYTES, 0);
        let fitted = fit_variants(&variants, budget);

        assert_eq!(fitted.items.len(), 2, "shrunk SV and small record both fit");
        assert!(!fitted.truncated);
        assert_eq!(fitted.records_shrunk, 1);
        assert!(total_len(&fitted.items) <= budget);

        let sv = &fitted.items[0];
        assert_eq!(sv["record_truncated"], true);
        assert_eq!(sv["info"]["ANN"]["omitted"], true);
        assert_eq!(sv["info"]["ANN"]["entries"], 22813);
        assert!(sv["info"]["ANN"]["bytes"].as_u64().unwrap() > 1_000_000);
        assert_eq!(sv["info"]["SVTYPE"], "DEL");
        assert_eq!(sv["info"]["END"], 224012315);
        assert_eq!(sv["filter"], json!(["SV_HARD"]));
        assert!(fitted.items[1].get("record_truncated").is_none());
    }

    #[test]
    fn many_samples_are_omitted_with_count() {
        let mut v = variant(10, "rsMany");
        for i in 0..3000 {
            let mut fields = HashMap::new();
            fields.insert("GT".to_string(), json!("0|1"));
            v.samples.insert(format!("SAMPLE{i:05}"), fields);
        }
        let fitted = fit_variants(&[v], 8 * 1024);
        let record = &fitted.items[0];
        assert_eq!(record["record_truncated"], true);
        assert_eq!(record["samples"], json!({}));
        assert_eq!(record["samples_omitted"], 3000);
        assert!(json_len(record) <= 8 * 1024);
    }

    #[test]
    fn largest_part_is_omitted_first() {
        // Samples (~60 KB) are larger than ANN (~20 KB); omitting samples alone
        // is enough, so ANN must be kept.
        let mut v = sv_with_ann(500);
        for i in 0..2000 {
            let mut fields = HashMap::new();
            fields.insert("GT".to_string(), json!("0/1"));
            v.samples.insert(format!("S{i}"), fields);
        }
        let fitted = fit_variants(&[v], 40 * 1024);
        let record = &fitted.items[0];
        assert_eq!(record["samples_omitted"], 2000);
        assert!(record["info"]["ANN"].is_array(), "ANN should be kept");
    }

    #[test]
    fn long_alleles_are_shortened() {
        let mut v = variant(10, "rsIns");
        v.alternate = vec!["ACGT".repeat(5000)];
        let fitted = fit_variants(&[v], 2048);
        let alt = fitted.items[0]["alternate"][0].as_str().unwrap();
        assert!(alt.ends_with("...(20000 bp)"), "got {alt}");
        assert!(json_len(&fitted.items[0]) <= 2048);
    }

    #[test]
    fn budget_never_drops_below_minimum() {
        assert_eq!(variant_budget(100, 0), MIN_VARIANT_BUDGET);
        assert_eq!(
            variant_budget(DEFAULT_MAX_RESPONSE_BYTES, 1000),
            DEFAULT_MAX_RESPONSE_BYTES - ENVELOPE_RESERVE - 1000
        );
    }
}

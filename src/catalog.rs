use crate::atomic; use crate::model::{Classification, OcrRecord, TranscriptEntry, VideoCandidate, VideoMetadata};
use anyhow::{bail,Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Definition {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    level: u8,
    terms: &'static [&'static str],
    prerequisites: &'static [&'static str],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<f64>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptRecord {
    pub id: String,
    pub name: String,
    pub category: String,
    pub level: u8,
    pub mention_count: usize,
    pub evidence_score: f64,
    pub evidence: Vec<Evidence>,
    pub prerequisites: Vec<String>,
}

fn defs_for_profile(_profile:&str)->Vec<Definition>{generic_defs()}

fn generic_defs()->Vec<Definition>{
    vec![
        Definition{id:"instruction",name:"Instruction",category:"education",level:1,terms:&["how to","tutorial","guide","explained","lesson","teaching"],prerequisites:&[]},
        Definition{id:"fundamentals",name:"Fundamentals",category:"education",level:1,terms:&["fundamentals","basics","beginner","foundation"],prerequisites:&["instruction"]},
        Definition{id:"technique",name:"Technique",category:"skills",level:2,terms:&["technique","method","approach","workflow","process"],prerequisites:&["fundamentals"]},
        Definition{id:"strategy",name:"Strategy",category:"decision-making",level:2,terms:&["strategy","tactics","planning","decision making","decision-making"],prerequisites:&["fundamentals"]},
        Definition{id:"mistakes",name:"Mistakes and Pitfalls",category:"analysis",level:2,terms:&["mistakes","common mistakes","pitfalls","avoid","errors"],prerequisites:&["fundamentals"]},
        Definition{id:"analysis",name:"Analysis",category:"analysis",level:2,terms:&["analysis","breakdown","review","case study","deep dive"],prerequisites:&["fundamentals"]},
        Definition{id:"tools",name:"Tools and Setup",category:"setup",level:2,terms:&["settings","setup","tools","configuration","workflow"],prerequisites:&["fundamentals"]},
        Definition{id:"advanced",name:"Advanced Concepts",category:"education",level:3,terms:&["advanced","expert","optimization","deep dive","advanced technique"],prerequisites:&["technique","strategy"]},
    ]
}

fn normalized(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect()
}

fn matches_term(text: &str, term: &str) -> bool {
    let haystack = format!(" {} ", normalized(text));
    let needle = format!(" {} ", normalized(term));
    haystack.contains(&needle)
}

fn evidence_for(text: &str, source: &str, time: Option<f64>, definitions: &[Definition], out: &mut HashMap<String, Vec<Evidence>>) {
    for definition in definitions {
        if definition.terms.iter().any(|term| matches_term(text, term)) {
            let entry = Evidence { source: source.into(), time, text: text.chars().take(500).collect() };
            let list = out.entry(definition.id.into()).or_default();
            if list.len() < 12 && !list.iter().any(|e| e.source == entry.source && e.time == entry.time && e.text == entry.text) {
                list.push(entry);
            }
        }
    }
}

pub fn extract(
    video: &VideoCandidate,
    metadata: &Option<VideoMetadata>,
    transcript: &[TranscriptEntry],
    ocr: &[OcrRecord],
    classification: &Classification,
    profile: &str,
) -> Vec<ConceptRecord> {
    let definitions = defs_for_profile(profile);
    let mut evidence = HashMap::<String, Vec<Evidence>>::new();
    evidence_for(&video.title, "title", None, &definitions, &mut evidence);
    if let Some(meta) = metadata {
        evidence_for(&meta.title, "metadata", None, &definitions, &mut evidence);
        evidence_for(&meta.description, "metadata", None, &definitions, &mut evidence);
    }
    for entry in transcript {
        evidence_for(&entry.text, "transcript", Some(entry.start), &definitions, &mut evidence);
    }
    for entry in ocr {
        evidence_for(&entry.text, "ocr", Some(entry.time), &definitions, &mut evidence);
    }

    definitions.into_iter().filter_map(|definition| {
        let ev = evidence.remove(definition.id)?;
        let mention_count = ev.len();
        let mut score = 0.0;
        for item in &ev {
            score += match item.source.as_str() {
                "transcript" => 3.0,
                "metadata" => 2.0,
                _ => 1.0,
            };
            let lower = item.text.to_lowercase();
            if lower.contains(" i ") || lower.starts_with("i ") || lower.contains(" my ") || lower.contains(" when i ") {
                score += 1.0;
            }
        }
        if classification.label == "educational" { score *= 1.15; }

        Some(ConceptRecord {
            id: definition.id.into(),
            name: definition.name.into(),
            category: definition.category.into(),
            level: definition.level,
            mention_count,
            evidence_score: score,
            evidence: ev,
            prerequisites: definition.prerequisites.iter().map(|s| (*s).into()).collect(),
        })
    }).collect()
}

pub fn build(output: &Path, records: &[crate::model::ClassificationRecord], profile: &str) -> Result<PathBuf> {
    let mut aggregate: HashMap<String, (String, String, u8, Vec<String>, usize, f64, usize)> = HashMap::new();

    for record in records {
        let path = output.join(&record.video.id).join("analysis.json");
        if !path.is_file() { continue; }
        let value: Value = serde_json::from_slice(&fs::read(&path)?)?;
        let concepts:Vec<ConceptRecord>=serde_json::from_value(value.get("concepts").cloned().unwrap_or_else(||Value::Array(Vec::new())))?;
        for concept in concepts {
            let id=concept.id;
            let name=concept.name;
            let category=concept.category;
            let level=concept.level;
            let mentions=concept.mention_count;
            let score=concept.evidence_score;
            if !score.is_finite()||score<0.0{bail!("invalid evidence score for concept {id}");}
            let prerequisites=concept.prerequisites;
            let entry = aggregate.entry(id.into()).or_insert((name, category, level, prerequisites, 0, 0.0, 0));
            entry.4 += mentions;
            entry.5 += score;
            entry.6 += 1;
        }
    }

    let mut concepts = aggregate.into_iter().map(|(id, (name, category, level, prerequisites, mentions, score, videos))| {
        let importance = score * (1.0 + (videos as f64).ln_1p()) + (mentions as f64).sqrt();
        serde_json::json!({
            "id": id,
            "name": name,
            "category": category,
            "level": level,
            "prerequisites": prerequisites,
            "videos": videos,
            "mentions": mentions,
            "evidenceScore": score,
            "importanceScore": importance
        })
    }).collect::<Vec<_>>();

    concepts.sort_by(|a, b| {
        b.get("importanceScore").and_then(Value::as_f64)
            .partial_cmp(&a.get("importanceScore").and_then(Value::as_f64))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.get("id").and_then(Value::as_str).cmp(&b.get("id").and_then(Value::as_str)))
    });

    let learning_order = prerequisite_learning_order(&concepts)?;
    let catalog = serde_json::json!({
        "schemaVersion": 3,
        "generatedAt": crate::registry::unix_seconds(),
        "project": "FrameForge",
        "profile": profile,
        "ranking": {
            "method": "evidence-weighted mention score plus cross-video coverage",
            "learningOrder": "prerequisite-aware topological ordering with level and importance tie-breaking"
        },
        "learningOrder": learning_order,
        "concepts": concepts
    });

    let path = output.join("research_catalog.json");
    atomic::write(&path,&serde_json::to_vec_pretty(&catalog)?)?;Ok(path)
}

fn prerequisite_learning_order(concepts: &[Value]) -> Result<Vec<String>> {
    let ids: HashSet<String> = concepts.iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let mut remaining: HashSet<String> = ids.clone();
    let mut order = Vec::new();

    while !remaining.is_empty() {
        let mut ready = remaining.iter().filter_map(|id| {
            let concept = concepts.iter().find(|c| c.get("id").and_then(Value::as_str) == Some(id.as_str()))?;
            let prerequisites = concept.get("prerequisites").and_then(Value::as_array).cloned().unwrap_or_default();
            let satisfied = prerequisites.iter().filter_map(Value::as_str).filter(|p| ids.contains(*p)).all(|p| order.iter().any(|done| done == p));
            if satisfied { Some(concept) } else { None }
        }).collect::<Vec<_>>();

        if ready.is_empty(){bail!("concept prerequisite graph contains a cycle or unsatisfiable prerequisite");}

        ready.sort_by(|a, b| {
            let level_a = a.get("level").and_then(Value::as_u64).unwrap_or(99);
            let level_b = b.get("level").and_then(Value::as_u64).unwrap_or(99);
            level_a.cmp(&level_b).then_with(|| {
                b.get("importanceScore").and_then(Value::as_f64)
                    .partial_cmp(&a.get("importanceScore").and_then(Value::as_f64))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }).then_with(|| a.get("id").and_then(Value::as_str).cmp(&b.get("id").and_then(Value::as_str)))
        });

        for concept in ready {
            let id = concept.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            if remaining.remove(&id) { order.push(id); }
        }
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_matching_respects_word_boundaries() {
        assert!(matches_term("well-defined process", "well defined"));
        assert!(!matches_term("processing", "process"));
    }
    #[test]
    fn learning_order_respects_prerequisites_and_is_deterministic() {
        let concepts = vec![
            serde_json::json!({"id":"advanced","level":4,"importanceScore":100.0,"prerequisites":["base"]}),
            serde_json::json!({"id":"zeta","level":1,"importanceScore":10.0,"prerequisites":[]}),
            serde_json::json!({"id":"alpha","level":1,"importanceScore":10.0,"prerequisites":[]}),
            serde_json::json!({"id":"base","level":1,"importanceScore":1.0,"prerequisites":[]}),
        ];
        let first = prerequisite_learning_order(&concepts).unwrap();
        assert_eq!(first, prerequisite_learning_order(&concepts).unwrap());
        assert!(first.iter().position(|id| id == "base").unwrap() < first.iter().position(|id| id == "advanced").unwrap());
        assert!(first.iter().position(|id| id == "alpha").unwrap() < first.iter().position(|id| id == "zeta").unwrap());
    }
}

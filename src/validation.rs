use anyhow::{bail,Context,Result};
use crate::model::{Classification,ConceptRecord,FrameRecord,OcrRecord,State,TranscriptEntry,VideoCandidate,VideoMetadata,VisualResult};
use serde_json::Value;
use std::collections::{HashMap,HashSet};
use std::fs;
use std::path::{Path,PathBuf};

#[derive(Debug)]
pub struct ValidationReport{pub videos_checked:usize}

pub fn validate_output(output:&Path)->Result<ValidationReport>{
    let manifest_path=output.join("manifest.json");
    let catalog_path=output.join("research_catalog.json");
    if !manifest_path.is_file(){bail!("missing {}",manifest_path.display());}
    if !catalog_path.is_file(){bail!("missing {}",catalog_path.display());}
    let manifest:Value=serde_json::from_slice(&fs::read(&manifest_path)?)?;
    if manifest.get("schemaVersion").and_then(Value::as_u64).unwrap_or(0)<2{bail!("manifest has an unsupported schema version");}
    let videos=manifest.get("videos").and_then(Value::as_array).context("manifest.videos must be an array")?;
    let catalog:Value=serde_json::from_slice(&fs::read(&catalog_path)?)?;
    if catalog.get("schemaVersion").and_then(Value::as_u64).unwrap_or(0)<2{bail!("research catalog has an unsupported schema version");}
    let learning_order=catalog.get("learningOrder").and_then(Value::as_array).context("research catalog has no learningOrder")?;
    let catalog_concepts=catalog.get("concepts").and_then(Value::as_array).context("research catalog has no concepts")?;
    let mut concept_ids=HashSet::new();
    for concept in catalog_concepts{
        let id=concept.get("id").and_then(Value::as_str).context("catalog concept is missing id")?;
        if !concept_ids.insert(id.to_string()){bail!("catalog contains duplicate concept {id}");}
        for p in concept.get("prerequisites").and_then(Value::as_array).context("catalog concept is missing prerequisites")?{
            let pid=p.as_str().context("catalog prerequisite must be a string")?;
            if !catalog_concepts.iter().any(|c|c.get("id").and_then(Value::as_str)==Some(pid)){bail!("concept {id} references unknown prerequisite {pid}");}
        }
    }
    let mut positions=HashMap::new();
    for (index,item) in learning_order.iter().enumerate(){
        let id=item.as_str().context("learningOrder entries must be strings")?;
        if !concept_ids.contains(id){bail!("learningOrder references unknown concept {id}");}
        if positions.insert(id,index).is_some(){bail!("learningOrder contains duplicate concept {id}");}
    }
    if positions.len()!=concept_ids.len(){bail!("learningOrder does not contain every catalog concept");}
    for concept in catalog_concepts{
        let id=concept.get("id").and_then(Value::as_str).unwrap();
        for p in concept.get("prerequisites").and_then(Value::as_array).unwrap(){
            let pid=p.as_str().unwrap();
            if positions[pid]>=positions[id]{bail!("learningOrder violates prerequisite {pid} -> {id}");}
        }
    }
    for video in videos{
        let id=video.get("id").and_then(Value::as_str).context("manifest video is missing id")?;
        let state_value=video.get("state").context("manifest video is missing state")?;
        let state:State=serde_json::from_value(state_value.clone()).context("manifest video has invalid state")?;
        if state.status!="complete"||state.stage!="complete"{bail!("video {id} is not complete");}
        let dir=output.join(id);
        if !is_safe_child(output,&dir)?{bail!("video {id} resolves outside the output directory");}
        let classification:Classification=read_required(&dir,"classification.json",id)?;
        let video_file:VideoCandidate=read_required(&dir,"video.json",id)?;
        let metadata:Option<VideoMetadata>=read_required(&dir,"metadata.json",id)?;
        let transcript:Vec<TranscriptEntry>=read_required(&dir,"transcript.json",id)?;
        let visual:VisualResult=read_required(&dir,"visual.json",id)?;
        let ocr:Vec<OcrRecord>=read_required(&dir,"ocr.json",id)?;
        let concepts:Vec<ConceptRecord>=read_required(&dir,"concepts.json",id)?;
        let analysis:Value=read_required_value(&dir,"analysis.json",id)?;
        if analysis.get("schemaVersion").and_then(Value::as_u64)!=Some(3){bail!("video {id} has an unsupported analysis schema");}
        let provenance=analysis.get("provenance").context("analysis is missing provenance")?;
        if provenance.get("videoId").and_then(Value::as_str)!=Some(id){bail!("video {id} has mismatched provenance");}
        if video_file.id!=id{bail!("video {id} has mismatched video.json identity");}
        if classification.profile_fingerprint.is_empty(){bail!("video {id} is missing its classification profile fingerprint");}
        if analysis.pointer("/provenance/sourceUrl").and_then(Value::as_str)!=Some(video_file.url.as_str()){bail!("video {id} has mismatched provenance URL");}
        if analysis.pointer("/pipeline/profileFingerprint").and_then(Value::as_str)!=Some(classification.profile_fingerprint.as_str()){bail!("video {id} has mismatched profile fingerprint");}
        if analysis.pointer("/pipeline/ocrEnabled").and_then(Value::as_bool)!=manifest.pointer("/pipeline/ocrEnabled").and_then(Value::as_bool){bail!("video {id} has mismatched OCR configuration");}
        if analysis.pointer("/pipeline/profile").and_then(Value::as_str)!=Some(manifest_profile){bail!("video {id} has mismatched profile");}
        if analysis.get("video")!=Some(&serde_json::to_value(&video_file)?){bail!("video {id} analysis/video mismatch");}
        if analysis.get("classification")!=Some(&serde_json::to_value(&classification)?){bail!("video {id} analysis/classification mismatch");}
        if analysis.get("metadata")!=Some(&serde_json::to_value(&metadata)?){bail!("video {id} analysis/metadata mismatch");}
        if analysis.get("transcript")!=Some(&serde_json::to_value(&transcript)?){bail!("video {id} analysis/transcript mismatch");}
        if analysis.get("ocr")!=Some(&serde_json::to_value(&ocr)?){bail!("video {id} analysis/ocr mismatch");}
        if analysis.get("concepts")!=Some(&serde_json::to_value(&concepts)?){bail!("video {id} analysis/concepts mismatch");}
        let coverage=analysis.get("coverage").context("analysis is missing coverage")?;
        if coverage.get("visualCoverageComplete").and_then(Value::as_bool)!=Some(true){bail!("video {id} has incomplete visual coverage");}
        let frames=analysis.get("frames").and_then(Value::as_array).context("analysis.frames must be an array")?;
        let expected=coverage.get("expectedVisualSamples").and_then(Value::as_u64).context("coverage missing expectedVisualSamples")? as usize;
        let sampled=coverage.get("sampledVisualSamples").and_then(Value::as_u64).context("coverage missing sampledVisualSamples")? as usize;
        if expected!=visual.expected_samples||sampled!=visual.sampled_samples||sampled!=frames.len()||((expected>0)&&sampled<expected){bail!("video {id} has inconsistent visual frame counts");}
        if state.frames!=sampled||state.ocr!=ocr.len(){bail!("video {id} state counters do not match artifacts");}
        let mut frame_paths=HashSet::new();
        for (index,frame) in frames.iter().enumerate(){
            let frame_index=frame.get("index").and_then(Value::as_u64).context("frame is missing index")? as usize;
            let time=frame.get("time").and_then(Value::as_f64).context("frame is missing time")?;
            let relative=frame.get("path").and_then(Value::as_str).context("frame is missing path")?;
            let path=Path::new(relative);
            if frame_index!=index||time!=index as f64||!time.is_finite()||time<0.0||path.is_absolute()||path.components().any(|c|matches!(c,std::path::Component::ParentDir|std::path::Component::RootDir|std::path::Component::Prefix(_)))||!relative.starts_with("frames/"){bail!("video {id} has unsafe frame metadata");}
            if !frame_paths.insert(relative.to_string()){bail!("video {id} has duplicate frame path {relative}");}
            let resolved=dir.join(path);
            if !resolved.is_file()||!is_safe_child(output,&resolved)?{bail!("video {id} is missing or escapes frame {}",relative);}
        }
        let concepts:Value=serde_json::from_slice(&fs::read(dir.join("concepts.json"))?)?;
        if !concepts.is_array(){bail!("video {id} concepts.json must be an array");}
        for concept in concepts.as_array().unwrap(){
            if concept.get("id").and_then(Value::as_str).is_none(){bail!("video {id} has a concept without an id");}
            if concept.get("evidence").and_then(Value::as_array).is_none(){bail!("video {id} has a concept without evidence");}
        }
    }
    Ok(ValidationReport{videos_checked:videos.len()})
}

fn read_required<T:for<'de>serde::Deserialize<'de>>(dir:&Path,name:&str,id:&str)->Result<T>{
    let path=dir.join(name);if !path.is_file(){bail!("video {id} is missing {name}");}
    serde_json::from_slice(&fs::read(path)?).with_context(||format!("video {id} has invalid {name}"))
}
fn read_required_value(dir:&Path,name:&str,id:&str)->Result<Value>{
    let path=dir.join(name);if !path.is_file(){bail!("video {id} is missing {name}");}
    serde_json::from_slice(&fs::read(path)?).with_context(||format!("video {id} has invalid {name}"))
}
fn is_safe_child(root:&Path,path:&Path)->Result<bool>{
    let root=fs::canonicalize(root)?;let path=fs::canonicalize(path)?;
    Ok(path.starts_with(&root))
}

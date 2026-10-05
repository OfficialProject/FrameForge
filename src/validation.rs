use anyhow::{bail,Context,Result};
use serde_json::Value;
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
    let concept_ids=catalog.get("concepts").and_then(Value::as_array).context("research catalog has no concepts")?.iter().filter_map(|c|c.get("id").and_then(Value::as_str)).collect::<std::collections::HashSet<_>>();
    for id in learning_order.iter().filter_map(Value::as_str){if !concept_ids.contains(id){bail!("learningOrder references unknown concept {id}");}}
    for video in videos{
        let id=video.get("id").and_then(Value::as_str).context("manifest video is missing id")?;
        let state=video.get("state").context("manifest video is missing state")?;
        if state.get("status").and_then(Value::as_str)!=Some("complete")||state.get("stage").and_then(Value::as_str)!=Some("complete"){bail!("video {id} is not complete");}
        let dir=output.join(id);
        for required in ["classification.json","video.json","metadata.json","transcript.json","visual.json","ocr.json","concepts.json","analysis.json"]{if !dir.join(required).is_file(){bail!("video {id} is missing {required}");}}
        let analysis:Value=serde_json::from_slice(&fs::read(dir.join("analysis.json"))?)?;
        if analysis.get("schemaVersion").and_then(Value::as_u64).unwrap_or(0)<2{bail!("video {id} has an unsupported analysis schema");}
        let provenance=analysis.get("provenance").context("analysis is missing provenance")?;
        if provenance.get("videoId").and_then(Value::as_str)!=Some(id){bail!("video {id} has mismatched provenance");}
        let coverage=analysis.get("coverage").context("analysis is missing coverage")?;
        if coverage.get("visualCoverageComplete").and_then(Value::as_bool)!=Some(true){bail!("video {id} has incomplete visual coverage");}
        let frames=analysis.get("frames").and_then(Value::as_array).context("analysis.frames must be an array")?;
        let sampled=coverage.get("sampledVisualSamples").and_then(Value::as_u64).unwrap_or(0) as usize;
        if sampled!=frames.len(){bail!("video {id} has inconsistent visual frame counts");}
        for frame in frames{
            let relative=frame.get("path").and_then(Value::as_str).context("frame is missing path")?;
            let path=PathBuf::from(relative);
            let resolved=if path.is_absolute(){path}else{dir.join(path)};
            if !resolved.is_file(){bail!("video {id} is missing frame {}",relative);}
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

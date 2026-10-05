use crate::model::{TranscriptEntry,VideoCandidate,VideoMetadata};
use crate::transcript::parse_vtt;
use anyhow::{anyhow,bail,Context,Result};
use serde_json::Value;
use std::fs;
use std::path::{Path,PathBuf};
use std::process::Command;
use std::time::{SystemTime,UNIX_EPOCH};

fn run(args:&[String])->Result<std::process::Output>{Command::new("yt-dlp").args(args).output().context("yt-dlp is required but was not found on PATH")}
fn validate_video_id(id:&str)->Result<()>{
    if id.len()!=11||!id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_'||c==b'-'){bail!("yt-dlp returned an invalid YouTube video id");}
    Ok(())
}
fn normalize_url(url:&str,id:&str)->String{if url.contains("youtube.com/watch"){format!("https://www.youtube.com/watch?v={id}")}else if url.len()==11{format!("https://www.youtube.com/watch?v={id}")}else{url.to_string()}}

pub fn discover_source(source:&str)->Result<Vec<VideoCandidate>>{
    let source=source.trim();
    let is_video=source.contains("youtube.com/watch")||source.contains("youtu.be/")||source.contains("youtube.com/shorts/");
    if is_video{return discover_video(source);}
    discover_channel(source)
}
fn discover_video(url:&str)->Result<Vec<VideoCandidate>>{
    let args=vec!["--no-warnings".into(),"--ignore-config".into(),"--no-playlist".into(),"--dump-single-json".into(),"--skip-download".into(),url.into()];
    let output=run(&args)?;
    if !output.status.success(){bail!("yt-dlp video discovery failed: {}",String::from_utf8_lossy(&output.stderr).trim());}
    let info:Value=serde_json::from_slice(&output.stdout).context("invalid yt-dlp video JSON")?;
    let id=info.get("id").and_then(Value::as_str).unwrap_or_default();
    validate_video_id(id)?;
    let title=info.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_string();
    let canonical_url=info.get("webpage_url").or_else(||info.get("original_url")).and_then(Value::as_str).unwrap_or(url);
    let source_name=info.get("channel").or_else(||info.get("uploader")).and_then(Value::as_str).unwrap_or(canonical_url).to_string();
    Ok(vec![VideoCandidate{id:id.into(),title,url:normalize_url(canonical_url,id),duration:info.get("duration").and_then(Value::as_f64),upload_date:info.get("upload_date").and_then(Value::as_str).map(str::to_owned),is_short:canonical_url.contains("/shorts/"),source_channels:vec![source_name]}])
}
pub fn discover_channel(channel:&str)->Result<Vec<VideoCandidate>>{
    let mut all=Vec::new();
    for tab in ["videos","shorts"]{
        let args=vec!["--no-warnings".into(),"--ignore-config".into(),"--flat-playlist".into(),"--dump-single-json".into(),"--skip-download".into(),"--ignore-errors".into(),format!("{}/{}",channel.trim_end_matches('/'),tab)];
        let output=run(&args)?;
        if !output.status.success(){bail!("yt-dlp discovery failed: {}",String::from_utf8_lossy(&output.stderr).trim());}
        let root:Value=serde_json::from_slice(&output.stdout).context("invalid yt-dlp playlist JSON")?;
        if let Some(entries)=root.get("entries").and_then(Value::as_array){for entry in entries{
            let id=entry.get("id").and_then(Value::as_str).unwrap_or_default();if id.is_empty(){continue;}validate_video_id(id)?;
            let title=entry.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_string();
            let raw=entry.get("webpage_url").or_else(||entry.get("url")).and_then(Value::as_str).unwrap_or(id);
            all.push(VideoCandidate{id:id.into(),title,url:normalize_url(raw,id),duration:entry.get("duration").and_then(Value::as_f64),upload_date:entry.get("upload_date").and_then(Value::as_str).map(str::to_owned),is_short:tab=="shorts",source_channels:vec![channel.to_string()]});
        }}
    }
    let mut unique=std::collections::HashMap::new();
    for v in all{unique.entry(v.id.clone()).and_modify(|e:&mut VideoCandidate|{e.is_short|=v.is_short;for source in &v.source_channels{if !e.source_channels.contains(source){e.source_channels.push(source.clone());}}}).or_insert(v);}
    Ok(unique.into_values().collect())
}
pub fn fetch_metadata_and_transcript(url:&str)->Result<(VideoMetadata,Vec<TranscriptEntry>)>{
    let dir=temp_dir("frameforge-meta")?;
    let args=vec!["--no-warnings".into(),"--ignore-config".into(),"--no-simulate".into(),"--skip-download".into(),"--dump-single-json".into(),"--write-subs".into(),"--write-auto-subs".into(),"--sub-format".into(),"vtt".into(),"--sub-langs".into(),"en.*".into(),"-o".into(),dir.join("captions").to_string_lossy().into_owned(),url.into()];
    let result=(||{let output=run(&args)?;let stdout=String::from_utf8_lossy(&output.stdout);let line=stdout.lines().rev().find(|l|l.trim_start().starts_with('{')).ok_or_else(||anyhow!("yt-dlp returned no metadata JSON"))?;let info:Value=serde_json::from_str(line).context("invalid video metadata JSON")?;let metadata=VideoMetadata{title:info.get("title").and_then(Value::as_str).unwrap_or_default().into(),description:info.get("description").and_then(Value::as_str).unwrap_or_default().into(),duration:info.get("duration").and_then(Value::as_f64),upload_date:info.get("upload_date").and_then(Value::as_str).map(str::to_owned)};let transcript=read_best_vtt(&dir)?.map(|v|parse_vtt(&v)).unwrap_or_default();Ok((metadata,transcript))})();let _=fs::remove_dir_all(&dir);result
}
pub fn download_video(url:&str,dir:&Path)->Result<PathBuf>{
    fs::create_dir_all(dir)?;if let Some(existing)=existing_source(dir){return Ok(existing);}
    let args=vec!["--no-warnings".into(),"--ignore-config".into(),"--no-playlist".into(),"--format".into(),"bv*+ba/b".into(),"--merge-output-format".into(),"mp4".into(),"-o".into(),dir.join("source.%(ext)s").to_string_lossy().into_owned(),url.into()];
    let result=run(&args)?;if !result.status.success(){bail!("video download failed: {}",String::from_utf8_lossy(&result.stderr).trim());}
    existing_source(dir).ok_or_else(||anyhow!("yt-dlp produced no usable source video"))
}
fn existing_source(dir:&Path)->Option<PathBuf>{fs::read_dir(dir).ok()?.filter_map(Result::ok).map(|e|e.path()).filter(|p|p.file_name().and_then(|n|n.to_str()).map(|n|n.starts_with("source.")).unwrap_or(false)&&p.extension().is_some()).find(|p|fs::metadata(p).map(|m|m.len()>0).unwrap_or(false)&&probe_media(p))}
fn probe_media(path:&Path)->bool{
    let Ok(output)=Command::new("ffprobe").args(["-v","error","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1"]).arg(path).output() else{return false;};
    if !output.status.success(){return false;}
    let Ok(duration)=String::from_utf8_lossy(&output.stdout).trim().parse::<f64>() else{return false;};
    duration.is_finite()&&duration>=0.0
}
fn read_best_vtt(dir:&Path)->Result<Option<String>>{let mut files=fs::read_dir(dir)?.filter_map(Result::ok).map(|e|e.path()).filter(|p|p.extension().and_then(|x|x.to_str())==Some("vtt")).collect::<Vec<_>>();files.sort();let p=files.iter().find(|p|p.file_name().and_then(|n|n.to_str()).map(|n|n.contains(".en")).unwrap_or(false)).cloned().or_else(||files.into_iter().next());p.map(fs::read_to_string).transpose().map_err(Into::into)}
fn temp_dir(prefix:&str)->Result<PathBuf>{let stamp=SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();let dir=std::env::temp_dir().join(format!("{prefix}-{stamp}-{}",std::process::id()));fs::create_dir_all(&dir)?;Ok(dir)}
#[cfg(test)]mod tests{use super::{normalize_url,validate_video_id};#[test]fn normalizes_id(){assert_eq!(normalize_url("dQw4w9WgXcQ","dQw4w9WgXcQ"),"https://www.youtube.com/watch?v=dQw4w9WgXcQ");}#[test]fn preserves_non_youtube_url(){assert_eq!(normalize_url("https://example.com/video","abc"),"https://example.com/video");}
#[test]fn rejects_unsafe_video_id(){assert!(validate_video_id("../escape").is_err());assert!(validate_video_id("safe-id_123").is_ok());}}

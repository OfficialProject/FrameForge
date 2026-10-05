use crate::model::{FrameRecord,OcrRecord,VisualResult};
use anyhow::{anyhow,Context,Result};
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn extract_exhaustive_1fps(video:&Path,frames_dir:&Path)->Result<VisualResult>{
    let duration=ffprobe_duration(video)?; let expected=duration.ceil() as usize;
    fs::create_dir_all(frames_dir)?;
    for entry in fs::read_dir(frames_dir)? { let path=entry?.path(); if path.extension().and_then(|e|e.to_str())==Some("jpg"){fs::remove_file(path)?;} }
    let pattern=frames_dir.join("frame-%06d.jpg");
    let output=Command::new("ffmpeg").args(["-hide_banner","-loglevel","error","-i"]).arg(video).args(["-vf","fps=1","-fps_mode","cfr","-q:v","2"]).arg(&pattern).output().context("ffmpeg is required but was not found on PATH")?;
    if !output.status.success(){return Err(anyhow!("ffmpeg frame extraction failed: {}",String::from_utf8_lossy(&output.stderr).trim()));}
    let mut paths=fs::read_dir(frames_dir)?.filter_map(Result::ok).map(|e|e.path()).filter(|p|p.extension().and_then(|e|e.to_str())==Some("jpg")).collect::<Vec<_>>();
    paths.sort();
    let frames=paths.iter().enumerate().map(|(index,path)|FrameRecord{index,time:index as f64,path:path.strip_prefix(frames_dir.parent().unwrap_or(frames_dir)).unwrap_or(path).to_string_lossy().replace('\\',"/")}).collect::<Vec<_>>();
    let warnings=if expected>0&&frames.len()<expected{vec![format!("Exhaustive coverage incomplete: {} of {} expected 1 FPS samples",frames.len(),expected)]}else{Vec::new()};
    Ok(VisualResult{expected_samples:expected,sampled_samples:frames.len(),frames,warnings})
}

fn ffprobe_duration(video:&Path)->Result<f64>{
    let output=Command::new("ffprobe").args(["-v","error","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1"]).arg(video).output().context("ffprobe is required but was not found on PATH")?;
    if !output.status.success(){return Err(anyhow!("ffprobe failed: {}",String::from_utf8_lossy(&output.stderr).trim()));}
    let duration:f64=String::from_utf8_lossy(&output.stdout).trim().parse().context("ffprobe returned an invalid duration")?;
    if !duration.is_finite()||duration<0.0{return Err(anyhow!("ffprobe returned an invalid duration: {duration}"));} Ok(duration)
}
pub fn tesseract_available()->bool{Command::new("tesseract").arg("--version").output().map(|o|o.status.success()).unwrap_or(false)}
pub fn ocr_frames(frames_dir:&Path,visual:&VisualResult)->Result<Vec<OcrRecord>>{
    if !tesseract_available(){return Ok(Vec::new());}
    Ok(visual.frames.par_iter().filter_map(|frame|{
        let path=frames_dir.parent().unwrap_or(frames_dir).join(&frame.path);
        let output=Command::new("tesseract").arg(&path).arg("stdout").arg("-l").arg("eng").arg("--psm").arg("6").output().ok()?;
        let text=String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.len()>3{Some(OcrRecord{time:frame.time,text})}else{None}
    }).collect())
}

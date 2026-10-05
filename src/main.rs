mod catalog;
mod classifier;
mod model;
mod progress;
mod registry;
mod transcript;
mod validation;
mod visual;
mod ytdlp;

use anyhow::{bail,Context,Result};
use clap::{Parser,Subcommand};
use classifier::Classifier;
use model::{ClassificationRecord,VideoCandidate};
use rayon::prelude::*;
use registry::{Registry,Stage};
use std::collections::HashMap;
use std::fs;
use std::path::{Path,PathBuf};
use std::sync::atomic::{AtomicUsize,Ordering};

const PIPELINE_VERSION:u32=3;

#[derive(Parser,Debug)]
#[command(name="frameforge",version,about="FrameForge video research and parsing pipeline")]
struct Cli{#[command(subcommand)]command:Command}
#[derive(Subcommand,Debug)]
enum Command{
    Run{#[arg(long)]sources:Option<PathBuf>,#[arg(long)]profile:Option<PathBuf>,#[arg(long)]root:Option<PathBuf>,#[arg(long)]output:Option<PathBuf>,#[arg(long)]force:bool},
    Discover{#[arg(long)]sources:Option<PathBuf>},
    Validate{#[arg(long)]output:Option<PathBuf>},
}
#[derive(Clone,Debug)]
struct Config{sources:PathBuf,profile:PathBuf,root:PathBuf,output:PathBuf,classify_concurrency:usize,research_concurrency:usize,force:bool,ocr:bool,keep_video:bool}
impl Config{
    fn from_args(sources:Option<PathBuf>,profile:Option<PathBuf>,root:Option<PathBuf>,output:Option<PathBuf>,force:bool)->Self{
        Self{
            sources:sources.unwrap_or_else(||PathBuf::from(std::env::var("FRAMEFORGE_SOURCES").unwrap_or_else(|_|"sources.txt".into()))),
            profile:profile.unwrap_or_else(||PathBuf::from(std::env::var("FRAMEFORGE_PROFILE").unwrap_or_else(|_|"profiles/default.json".into()))),
            root:root.unwrap_or_else(||PathBuf::from(std::env::var("FRAMEFORGE_ROOT").unwrap_or_else(|_|".frameforge".into()))),
            output:output.unwrap_or_else(||PathBuf::from(std::env::var("FRAMEFORGE_OUTPUT").unwrap_or_else(|_|"frameforge-output".into()))),
            classify_concurrency:env_usize("FRAMEFORGE_CLASSIFY_CONCURRENCY",6).clamp(1,16),
            research_concurrency:env_usize("FRAMEFORGE_RESEARCH_CONCURRENCY",2).clamp(1,4),
            force:force||std::env::var("FRAMEFORGE_FORCE_REFRESH").as_deref()==Ok("1"),
            ocr:std::env::var("FRAMEFORGE_OCR").as_deref()==Ok("1"),
            keep_video:std::env::var("FRAMEFORGE_KEEP_VIDEO").as_deref()==Ok("1"),
        }
    }
}
fn env_usize(name:&str,default:usize)->usize{std::env::var(name).ok().and_then(|v|v.parse().ok()).unwrap_or(default)}
fn main()->Result<()>{
    match Cli::parse().command{
        Command::Run{sources,profile,root,output,force}=>run(Config::from_args(sources,profile,root,output,force)),
        Command::Discover{sources}=>{
            let path=sources.unwrap_or_else(||PathBuf::from(std::env::var("FRAMEFORGE_SOURCES").unwrap_or_else(|_|"sources.txt".into())));
            for source in read_sources(&path)?{println!("{}: {} candidates",source,ytdlp::discover_source(&source)?.len());}
            Ok(())
        },
        Command::Validate{output}=>{
            let output=output.unwrap_or_else(||PathBuf::from(std::env::var("FRAMEFORGE_OUTPUT").unwrap_or_else(|_|"frameforge-output".into())));
            let report=validation::validate_output(&output)?;
            println!("VALIDATION PASSED: {} videos checked",report.videos_checked);
            Ok(())
        },
    }
}
fn run(config:Config)->Result<()>{
    fs::create_dir_all(&config.root)?;fs::create_dir_all(&config.output)?;
    if config.ocr&&!visual::tesseract_available(){bail!("OCR is enabled but Tesseract is unavailable. Install Tesseract or set FRAMEFORGE_OCR=0.");}
    let sources=read_sources(&config.sources)?;if sources.is_empty(){bail!("No research sources found.");}
    let classifier=Classifier::new(config.root.join("classifications"),config.force,config.profile.clone())?;
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                         FRAMEFORGE                          ║");
    println!("║ configurable video research • evidence • parsing           ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("Profile: {} | Shorts eligible | recall-first filtering | exhaustive 1 FPS visual research",classifier.profile_name());
    println!();
    let mut all=Vec::new();
    for source in &sources{
        let videos=ytdlp::discover_source(source).with_context(||format!("failed to discover {source}"))?;
        println!("{source}: discovered {} candidates",videos.len());all.extend(videos);
    }
    let mut unique=HashMap::<String,VideoCandidate>::new();
    for video in all{
        unique.entry(video.id.clone()).and_modify(|v|{
            v.is_short|=video.is_short;
            for source in &video.source_channels{if !v.source_channels.contains(source){v.source_channels.push(source.clone());}}
        }).or_insert(video);
    }
    let candidates:Vec<_>=unique.into_values().collect();
    println!("Unique candidates: {}",candidates.len());
    let done=AtomicUsize::new(0);
    let pool=rayon::ThreadPoolBuilder::new().num_threads(config.classify_concurrency).build()?;
    let classified:Vec<ClassificationRecord>=pool.install(||candidates.par_iter().map(|v|{
        let r=classifier.classify(v);let n=done.fetch_add(1,Ordering::Relaxed)+1;progress::line("Classifying",n,candidates.len());r
    }).collect::<Result<Vec<_>>>())?;
    println!();
    let retained:Vec<_>=classified.into_iter().filter(|r|r.classification.keep_for_research).collect();
    let rejected=candidates.len()-retained.len();
    let shorts=retained.iter().filter(|r|r.video.is_short).count();
    let uncertain=retained.iter().filter(|r|r.classification.label=="uncertain").count();
    println!("Retained: {} | rejected: {} | Shorts: {} | uncertain: {}",retained.len(),rejected,shorts,uncertain);
    let registry=Registry::new(config.root.join("state"))?;
    let done=AtomicUsize::new(0);
    let pool=rayon::ThreadPoolBuilder::new().num_threads(config.research_concurrency).build()?;
    pool.install(||retained.par_iter().map(|r|{
        let result=research_one(r,&config,&registry);let n=done.fetch_add(1,Ordering::Relaxed)+1;progress::line("Researching",n,retained.len());result
    }).collect::<Result<Vec<_>>>())?;
    println!();
    let catalog=catalog::build(&config.output,&retained,classifier.profile_name())?;
    let manifest=registry::build_manifest(&config.output,&sources,&retained,&registry,config.ocr,&catalog,classifier.profile_name())?;
    let report=validation::validate_output(&config.output)?;
    println!("══════════════════════ RESEARCH COMPLETE ═════════════════════");
    println!("Discovered: {} | Retained: {} | Rejected: {}",candidates.len(),retained.len(),rejected);
    println!("Shorts retained: {} | Uncertain retained: {}",shorts,uncertain);
    println!("Validated: {} videos | Catalog: {}",report.videos_checked,catalog.display());
    println!("Output: {}",manifest.display());
    Ok(())
}
fn research_one(record:&ClassificationRecord,config:&Config,registry:&Registry)->Result<()>{
    let id=&record.video.id;if !config.force&&registry.is_complete(id){return Ok(());}
    let dir=config.output.join(id);let frames_dir=dir.join("frames");fs::create_dir_all(&frames_dir)?;
    let previous=registry.start(id)?;
    let result:Result<()>=(||{
        let(metadata,transcript)=if previous.is_some()&&dir.join("metadata.json").is_file()&&dir.join("transcript.json").is_file(){(read_json(&dir.join("metadata.json"))?,read_json(&dir.join("transcript.json"))?)}else{fetch_evidence(record,&dir)?};
        write_json(&dir.join("classification.json"),&record.classification)?;
        write_json(&dir.join("video.json"),&record.video)?;
        registry.mark_stage(id,Stage::Metadata)?;
        let visual_path=dir.join("visual.json");
        let cached_visual=if !config.force&&registry.stage_at_least(id,Stage::Visual)&&valid_visual_cache(&visual_path,&frames_dir)?{Some(read_json(&visual_path)?)}else{None};
        let visual=if let Some(visual)=cached_visual{visual}else{
            let source=ytdlp::download_video(&record.video.url,&dir)?;registry.mark_stage(id,Stage::Downloaded)?;
            let visual=visual::extract_exhaustive_1fps(&source,&frames_dir)?;write_json(&visual_path,&visual)?;registry.mark_stage(id,Stage::Visual)?;visual
        };
        let ocr_path=dir.join("ocr.json");
        let ocr=if !config.force&&config.ocr&&registry.stage_at_least(id,Stage::Ocr)&&ocr_path.is_file(){read_json(&ocr_path)?}else if config.ocr{
            let ocr=visual::ocr_frames(&frames_dir,&visual)?;write_json(&ocr_path,&ocr)?;registry.mark_stage(id,Stage::Ocr)?;ocr
        }else{write_json(&ocr_path,&Vec::<model::OcrRecord>::new())?;registry.mark_stage(id,Stage::Ocr)?;Vec::new()};
        let mut warnings=visual.warnings.clone();
        if config.ocr&&!visual::tesseract_available(){warnings.push("Tesseract is unavailable; OCR was skipped.".into());}
        if dir.join("evidence_warning.txt").is_file(){warnings.push(fs::read_to_string(dir.join("evidence_warning.txt"))?);}
        let concepts=catalog::extract(&record.video,&metadata,&transcript,&ocr,&record.classification,config.profile.file_stem().and_then(|s|s.to_str()).unwrap_or("default"));
        write_json(&dir.join("concepts.json"),&concepts)?;
        let research=serde_json::json!({
            "schemaVersion":3,
            "pipeline":{"name":"FrameForge","version":PIPELINE_VERSION,"profile":config.profile.file_stem().and_then(|s|s.to_str()).unwrap_or("default"),"classificationVersion":record.classification.classifier_version,"visualSampling":"exhaustive_1fps","ocrEnabled":config.ocr,"generatedAt":registry::unix_seconds(),"tools":tool_versions()},
            "provenance":{"sourceUrl":record.video.url,"videoId":record.video.id,"sourceChannels":record.video.source_channels,"evidence":{"title":record.video.title,"metadataFile":"metadata.json","transcriptFile":"transcript.json","frameDirectory":"frames/","ocrFile":"ocr.json","conceptsFile":"concepts.json"}},
            "video":record.video,"metadata":metadata,"classification":record.classification,
            "coverage":{"mode":"exhaustive","visualSamplingFps":1,"expectedVisualSamples":visual.expected_samples,"sampledVisualSamples":visual.sampled_samples,"visualCoverageComplete":visual.expected_samples==0||visual.sampled_samples>=visual.expected_samples},
            "frames":visual.frames,"ocr":ocr,"transcript":transcript,"concepts":concepts,"warnings":warnings
        });
        write_json(&dir.join("analysis.json"),&research)?;
        if !config.keep_video{if let Ok(entries)=fs::read_dir(&dir){for entry in entries.flatten(){let path=entry.path();if path.file_name().and_then(|n|n.to_str()).map(|n|n.starts_with("source.")).unwrap_or(false){let _=fs::remove_file(path);}}}}
        registry.mark_complete(id,visual.sampled_samples,ocr.len())?;
        Ok(())
    })();
    if let Err(ref e)=result{registry.mark_failed(id,&e.to_string())?;}result
}
fn fetch_evidence(record:&ClassificationRecord,dir:&Path)->Result<(Option<model::VideoMetadata>,Vec<model::TranscriptEntry>)>{
    let(metadata,transcript)=if record.metadata.is_some()||!record.transcript.is_empty(){(record.metadata.clone(),record.transcript.clone())}else{match ytdlp::fetch_metadata_and_transcript(&record.video.url){Ok((metadata,transcript))=>(Some(metadata),transcript),Err(error)=>{fs::write(dir.join("evidence_warning.txt"),format!("Metadata/transcript retrieval failed: {error}"))?;(None,Vec::new())}}};
    write_json(&dir.join("metadata.json"),&metadata)?;write_json(&dir.join("transcript.json"),&transcript)?;Ok((metadata,transcript))
}
fn valid_visual_cache(visual_path:&Path,frames_dir:&Path)->Result<bool>{
    if !visual_path.is_file(){return Ok(false);}let visual:model::VisualResult=read_json(visual_path)?;
    if visual.expected_samples>0&&visual.sampled_samples<visual.expected_samples{return Ok(false);}
    if visual.sampled_samples!=visual.frames.len(){return Ok(false);}
    Ok(visual.frames.iter().all(|frame|frames_dir.parent().unwrap_or(frames_dir).join(&frame.path).is_file()))
}
fn tool_versions()->serde_json::Value{serde_json::json!({"yt-dlp":command_version("yt-dlp",&["--version"]),"ffmpeg":command_version("ffmpeg",&["-version"]),"ffprobe":command_version("ffprobe",&["-version"]),"tesseract":command_version("tesseract",&["--version"])})}
fn command_version(command:&str,args:&[&str])->Option<String>{std::process::Command::new(command).args(args).output().ok().filter(|o|o.status.success()).and_then(|o|String::from_utf8(o.stdout).ok()).and_then(|s|s.lines().next().map(str::to_owned))}
fn write_json<T:serde::Serialize>(path:&Path,value:&T)->Result<()>{fs::write(path,serde_json::to_vec_pretty(value)?)?;Ok(())}
fn read_json<T:for<'de>serde::Deserialize<'de>>(path:&Path)->Result<T>{Ok(serde_json::from_slice(&fs::read(path)?)?)}
fn read_sources(path:&Path)->Result<Vec<String>>{let text=fs::read_to_string(path).with_context(||format!("cannot read {}",path.display()))?;Ok(text.lines().map(str::trim).filter(|l|!l.is_empty()&&!l.starts_with('#')).map(ToOwned::to_owned).collect())}

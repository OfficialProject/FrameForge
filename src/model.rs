use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct VideoCandidate{
    pub id:String,pub title:String,pub url:String,
    #[serde(default)] pub duration:Option<f64>,
    #[serde(default)] pub upload_date:Option<String>,
    #[serde(default)] pub is_short:bool,
    #[serde(default)] pub source_channels:Vec<String>,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct VideoMetadata{pub title:String,#[serde(default)]pub description:String,#[serde(default)]pub duration:Option<f64>,#[serde(default)]pub upload_date:Option<String>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct TranscriptEntry{pub start:f64,pub end:f64,pub text:String}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Classification{
    pub label:String,pub score:i32,pub reasons:Vec<String>,pub transcript_used:bool,
    pub keep_for_research:bool,pub classifier_version:u32,
    #[serde(default)] pub profile:String,
    #[serde(default)] pub profile_fingerprint:String,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct ClassificationRecord{pub video:VideoCandidate,pub metadata:Option<VideoMetadata>,pub transcript:Vec<TranscriptEntry>,pub classification:Classification}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct FrameRecord{pub index:usize,pub time:f64,pub path:String}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct OcrRecord{pub time:f64,pub text:String}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct VisualResult{pub expected_samples:usize,pub sampled_samples:usize,pub frames:Vec<FrameRecord>,pub warnings:Vec<String>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct State{pub status:String,pub stage:String,pub attempts:usize,pub started_at:u64,pub updated_at:u64,#[serde(default)]pub frames:usize,#[serde(default)]pub ocr:usize,#[serde(default)]pub error:Option<String>}

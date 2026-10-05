use crate::atomic; use crate::model::{ClassificationRecord,State};
use anyhow::Result;
use std::fs;
use std::path::{Path,PathBuf};
use std::time::{SystemTime,UNIX_EPOCH};

#[derive(Clone,Copy,Debug,Eq,Ord,PartialEq,PartialOrd)]
pub enum Stage{Metadata,Downloaded,Visual,Ocr,Complete}
impl Stage{
    fn as_str(self)->&'static str{match self{Self::Metadata=>"metadata",Self::Downloaded=>"downloaded",Self::Visual=>"visual",Self::Ocr=>"ocr",Self::Complete=>"complete"}}
    fn from_str(value:&str)->Option<Self>{Some(match value{"metadata"=>Self::Metadata,"downloaded"=>Self::Downloaded,"visual"=>Self::Visual,"ocr"=>Self::Ocr,"complete"=>Self::Complete,_=>return None})}
}
pub struct Registry{root:PathBuf}
impl Registry{
    pub fn new(root:PathBuf)->Result<Self>{fs::create_dir_all(&root)?;Ok(Self{root})}
    fn path(&self,id:&str)->PathBuf{self.root.join(format!("{id}.json"))}
    fn load(&self,id:&str)->Option<State>{fs::read_to_string(self.path(id)).ok().and_then(|s|serde_json::from_str::<State>(&s).ok())}
    pub fn is_complete(&self,id:&str)->bool{self.load(id).map(|s|s.status=="complete"&&s.stage=="complete").unwrap_or(false)}
    pub fn stage_at_least(&self,id:&str,stage:Stage)->bool{self.load(id).and_then(|s|Stage::from_str(&s.stage)).map(|current|current>=stage).unwrap_or(false)}
    pub fn start(&self,id:&str)->Result<Option<State>>{let previous=self.load(id);let attempts=previous.as_ref().map(|s|s.attempts).unwrap_or(0)+1;let now=unix_seconds();self.save_state(id,State{status:"processing".into(),stage:previous.as_ref().and_then(|s|Stage::from_str(&s.stage)).map(Stage::as_str).unwrap_or("metadata").into(),attempts,started_at:previous.as_ref().map(|s|s.started_at).unwrap_or(now),updated_at:now,frames:previous.as_ref().map(|s|s.frames).unwrap_or(0),ocr:previous.as_ref().map(|s|s.ocr).unwrap_or(0),error:None})?;Ok(previous)}
    pub fn mark_stage(&self,id:&str,stage:Stage)->Result<()>{let mut state=self.load(id).unwrap_or(State{status:"processing".into(),stage:"metadata".into(),attempts:1,started_at:unix_seconds(),updated_at:unix_seconds(),frames:0,ocr:0,error:None});state.status="processing".into();state.stage=stage.as_str().into();state.updated_at=unix_seconds();self.save_state(id,state)}
    pub fn mark_complete(&self,id:&str,frames:usize,ocr:usize)->Result<()>{let mut state=self.load(id).unwrap_or(State{status:"processing".into(),stage:"ocr".into(),attempts:1,started_at:unix_seconds(),updated_at:unix_seconds(),frames:0,ocr:0,error:None});state.status="complete".into();state.stage="complete".into();state.updated_at=unix_seconds();state.frames=frames;state.ocr=ocr;state.error=None;self.save_state(id,state)}
    pub fn mark_failed(&self,id:&str,error:&str)->Result<()>{let mut state=self.load(id).unwrap_or(State{status:"failed".into(),stage:"metadata".into(),attempts:1,started_at:unix_seconds(),updated_at:unix_seconds(),frames:0,ocr:0,error:None});state.status="failed".into();state.updated_at=unix_seconds();state.error=Some(error.into());self.save_state(id,state)}
    fn save_state(&self,id:&str,state:State)->Result<()>{let path=self.path(id);atomic::write(&path,&serde_json::to_vec_pretty(&state)?)?;Ok(())}
}
pub fn build_manifest(output:&Path,sources:&[String],records:&[ClassificationRecord],registry:&Registry,ocr_enabled:bool,catalog:&Path,profile:&str)->Result<PathBuf>{
    let videos=records.iter().map(|r|serde_json::json!({
        "id":r.video.id.clone(),
        "url":r.video.url.clone(),
        "title":r.video.title.clone(),
        "isShort":r.video.is_short,
        "sourceChannels":r.video.source_channels.clone(),
        "classification":r.classification.clone(),
        "state":registry.load(&r.video.id)
    })).collect::<Vec<_>>();
    let catalog_name=catalog.file_name().and_then(|name|name.to_str()).unwrap_or("research_catalog.json");
    let manifest=serde_json::json!({"schemaVersion":3,"generatedAt":unix_seconds(),"project":"FrameForge","sources":sources,"profile":profile,"pipeline":{"version":3,"classification":"profile-driven recall-first classifier","visualSampling":"exhaustive 1fps","maxFrames":null,"shortsEligible":true,"uncertainCandidatesRetained":true,"ocrEnabled":ocr_enabled,"catalog":"research_catalog.json"},"catalogPath":catalog_name,"videos":videos});
    let path=output.join("manifest.json");atomic::write(&path,&serde_json::to_vec_pretty(&manifest)?)?;Ok(path)
}
pub fn unix_seconds()->u64{SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_secs()).unwrap_or(0)}
#[cfg(test)]mod tests{use super::{Registry,Stage};use std::time::{SystemTime,UNIX_EPOCH};#[test]fn state_advances_and_resumes(){let root=std::env::temp_dir().join(format!("frameforge-registry-test-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));let registry=Registry::new(root.clone()).unwrap();assert!(!registry.is_complete("demo"));assert!(registry.start("demo").unwrap().is_none());registry.mark_stage("demo",Stage::Downloaded).unwrap();assert!(registry.stage_at_least("demo",Stage::Metadata));assert!(registry.stage_at_least("demo",Stage::Downloaded));assert!(!registry.stage_at_least("demo",Stage::Visual));registry.mark_complete("demo",42,7).unwrap();assert!(registry.is_complete("demo"));let state:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(root.join("demo.json")).unwrap()).unwrap();assert_eq!(state["frames"],42);assert_eq!(state["ocr"],7);assert!(state["attempts"].as_u64().unwrap()>=1);let _=std::fs::remove_dir_all(root);}}
use crate::atomic; use crate::model::{Classification,ClassificationRecord,VideoCandidate};
use crate::ytdlp;
use anyhow::{Context,Result};
use serde::Deserialize;
use sha2::{Digest,Sha256};
use std::fs;
use std::path::PathBuf;

const VERSION:u32=3;

#[derive(Clone,Debug,Deserialize)]
struct Profile{
    name:String,
    #[allow(dead_code)]
    description:String,
    classification:Rules,
}
#[derive(Clone,Debug,Deserialize)]
struct Rules{
    educational_threshold:i32,
    short_educational_threshold:i32,
    non_educational_threshold:i32,
    short_penalty:i32,
    positive:Vec<(String,i32,String)>,
    negative:Vec<(String,i32,String)>,
}

pub struct Classifier{root:PathBuf,force:bool,profile:Profile,profile_fingerprint:String}

impl Classifier{
    pub fn new(root:PathBuf,force:bool,profile_path:PathBuf)->Result<Self>{
        fs::create_dir_all(&root)?;
        let bytes=fs::read(&profile_path).with_context(||format!("cannot read research profile {}",profile_path.display()))?;
        let profile:Profile=serde_json::from_slice(&bytes)
            .with_context(||format!("cannot parse research profile {}",profile_path.display()))?;
        validate_profile(&profile)?;
        let profile_fingerprint=format!("{:x}",Sha256::digest(&bytes));
        Ok(Self{root,force,profile,profile_fingerprint})
    }
    pub fn profile_name(&self)->&str{&self.profile.name}

    pub fn classify(&self,video:&VideoCandidate)->Result<ClassificationRecord>{
        let path=self.root.join(format!("{}.json",video.id));
        if !self.force {
            if let Ok(text)=fs::read_to_string(&path) {
                if let Ok(r)=serde_json::from_str::<ClassificationRecord>(&text) {
                    if r.video.title==video.title&&r.video.url==video.url&&r.video.is_short==video.is_short&&r.classification.classifier_version==VERSION&&r.classification.profile==self.profile.name&&r.classification.profile_fingerprint==self.profile_fingerprint{return Ok(r);}
                }
            }
        }
        match self.quick_title(&video.title,video.is_short){
            Decision::Educational=>self.finish(video,None,Vec::new(),"educational",10,vec!["strong title evidence".into()],false),
            Decision::NonEducational=>self.finish(video,None,Vec::new(),"non_educational",-10,vec!["strong negative title evidence".into()],false),
            Decision::Uncertain=>match ytdlp::fetch_metadata_and_transcript(&video.url){
                Ok((metadata,transcript))=>{
                    let body=transcript.iter().map(|e|e.text.as_str()).collect::<Vec<_>>().join(" ");
                    let(label,score,reasons)=self.score_text(&format!("{}\n{}\n{}",metadata.title,metadata.description,body),video.is_short);
                    self.finish(video,Some(metadata),transcript,label,score,reasons,true)
                }
                Err(e)=>self.finish(video,None,Vec::new(),"uncertain",0,vec![format!("classification evidence unavailable: {e}")],false),
            },
        }
    }

    fn finish(&self,video:&VideoCandidate,metadata:Option<crate::model::VideoMetadata>,transcript:Vec<crate::model::TranscriptEntry>,label:&str,score:i32,reasons:Vec<String>,transcript_used:bool)->Result<ClassificationRecord>{
        let record=ClassificationRecord{
            video:video.clone(),metadata,transcript,
            classification:Classification{label:label.into(),score,reasons,transcript_used,keep_for_research:label!="non_educational",classifier_version:VERSION,profile:self.profile.name.clone(),profile_fingerprint:self.profile_fingerprint.clone()}
        };
        let path=self.root.join(format!("{}.json",video.id));
        atomic::write(&path,&serde_json::to_vec_pretty(&record)?)?;
        Ok(record)
    }

    fn quick_title(&self,title:&str,short:bool)->Decision{
        let(score,positive_count,negative_count,_)=self.score_components(title);
        let threshold=if short{self.profile.classification.short_educational_threshold}else{self.profile.classification.educational_threshold};
        if score>=threshold&&positive_count>0&&negative_count==0{Decision::Educational}
        else if score<=self.profile.classification.non_educational_threshold||(negative_count>=2&&score<=0){Decision::NonEducational}
        else{Decision::Uncertain}
    }

    fn score_text(&self,text:&str,short:bool)->(&'static str,i32,Vec<String>){
        let(mut score,positive_count,negative_count,mut reasons)=self.score_components(text);
        if text.split_whitespace().count()>150{score+=2;reasons.push("substantial textual evidence".into());}
        if short{score+=self.profile.classification.short_penalty;reasons.push("Shorts require stronger evidence".into());}
        let threshold=self.profile.classification.educational_threshold;
        let label=if score>=threshold&&positive_count>0&&negative_count==0{"educational"}
        else if score<=self.profile.classification.non_educational_threshold||(negative_count>positive_count&&score<=0){"non_educational"}
        else{"uncertain"};
        if positive_count==0&&negative_count==0{reasons.push("no deterministic topic evidence".into());}
        (label,score,reasons)
    }

    fn score_components(&self,text:&str)->(i32,usize,usize,Vec<String>){
        let lower=text.to_lowercase();
        let(mut score,mut positive_count,mut negative_count,mut reasons)=(0,0,0,Vec::new());
        for(term,weight,reason)in &self.profile.classification.positive{if matches_term(&lower,term){score+=*weight;positive_count+=1;reasons.push(reason.clone());}}
        for(term,weight,reason)in &self.profile.classification.negative{if matches_term(&lower,term){score+=*weight;negative_count+=1;reasons.push(format!("negative: {reason}"));}}
        (score,positive_count,negative_count,reasons)
    }
}

fn matches_term(text:&str,term:&str)->bool{
    let haystack=format!(" {} ",text.chars().map(|c|if c.is_ascii_alphanumeric(){c}else{' '}).collect::<String>());
    let needle=format!(" {} ",term.to_lowercase().chars().map(|c|if c.is_ascii_alphanumeric(){c}else{' '}).collect::<String>());
    haystack.contains(&needle)
}

fn validate_profile(profile:&Profile)->Result<()>{
    if profile.name.trim().is_empty(){return Err(anyhow::anyhow!("research profile name cannot be empty"));}
    let mut seen=std::collections::HashSet::new();
    for (term,_,_) in profile.classification.positive.iter().chain(profile.classification.negative.iter()){
        if term.trim().is_empty(){return Err(anyhow::anyhow!("research profile contains an empty keyword"));}
        let normalized=term.to_lowercase().chars().map(|c|if c.is_ascii_alphanumeric(){c}else{' '}).collect::<String>();
        if normalized.split_whitespace().next().is_none(){return Err(anyhow::anyhow!("research profile keyword contains no searchable characters: {term:?}"));}
        if !seen.insert(normalized){return Err(anyhow::anyhow!("research profile contains duplicate keywords: {term:?}"));}
    }
    Ok(())
}

#[derive(Clone,Copy)]enum Decision{Educational,NonEducational,Uncertain}

#[cfg(test)]
mod tests{
    use super::*;
    fn classifier()->Classifier{
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("profiles/default.json");
        Classifier::new(std::env::temp_dir().join(format!("frameforge-classifier-test-{}",std::process::id())),true,path).unwrap()
    }
    #[test]fn educational(){assert!(matches!(classifier().quick_title("How to improve your workflow",false),Decision::Educational));}
    #[test]fn montage(){assert!(matches!(classifier().quick_title("Best highlights montage",false),Decision::NonEducational));}
    #[test]fn uncertain(){let(label,_,_)=classifier().score_text("Thoughts and observations",false);assert_eq!(label,"uncertain");}
    #[test]fn avoids_substring_false_positive(){assert!(!matches_term("claiming a result","aim"));assert!(matches_term("aim fundamentals","aim"));}
    #[test]fn rejects_empty_profile_keyword(){let mut c=classifier();c.profile.classification.positive[0].0.clear();assert!(validate_profile(&c.profile).is_err());}
}

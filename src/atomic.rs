use anyhow::Result;
use std::fs::{self,OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime,UNIX_EPOCH};

pub fn write(path:&Path,data:&[u8])->Result<()>{
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let name=path.file_name().and_then(|n|n.to_str()).unwrap_or("frameforge.json");
    let temp=path.with_file_name(format!(".{name}.tmp.{}.{}",std::process::id(),stamp));
    let mut file=OpenOptions::new().write(true).create_new(true).open(&temp)?;
    if let Err(error)=file.write_all(data).and_then(|_|file.sync_all()){
        let _=fs::remove_file(&temp);
        return Err(error.into());
    }
    drop(file);
    if let Err(error)=fs::rename(&temp,path){
        let _=fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write;
    use std::fs;
    use std::time::{SystemTime,UNIX_EPOCH};
    #[test]
    fn writes_and_replaces() {
        let path=std::env::temp_dir().join(format!("frameforge-atomic-test-{}.json",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        write(&path,b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(),b"first");
        write(&path,b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(),b"second");
        let _=fs::remove_file(path);
    }
}

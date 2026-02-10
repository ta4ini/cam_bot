use std::{
    fs::{self, File}, io::{BufReader, Read}, path::{Path, PathBuf}
};
use serde::{Deserialize, de::DeserializeOwned};
use chrono::Local;

pub fn get_project_root() -> PathBuf {
    std::env::current_dir().expect("Project folder not found")
}

pub fn create_dir(folder_path: &str) -> std::io::Result<()> {
    let path = Path::new(folder_path);
    fs::create_dir_all(path)?;

    Ok(())
}

pub fn get_motion_folder_path() -> PathBuf {
    let root = get_project_root();
    let folder = std::env::var("MOTION_FOLDER").unwrap_or_else(|_| "motion".into());
    root.join(folder)
        .join(Local::now().format("%Y-%m-%d").to_string())
}

pub fn get_last_image(prefix: &str, camera_id: &str) -> Option<PathBuf> {
    let path_folder = get_motion_folder_path();
    if !path_folder.is_dir() {
        return None;
    }

    let mut latest_file = None;
    let mut latest_time = std::time::SystemTime::UNIX_EPOCH;

    for entry in path_folder
        .read_dir()
        .expect("Read dir call faild")
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename_osstring = entry.file_name();
        if let Some(filename_str) = filename_osstring.to_str()
            && (!filename_str.contains(prefix) || !filename_str.contains(camera_id))
        {
            continue;
        }

        if let Ok(metadata) = entry.metadata() {
            let modified = metadata.modified().unwrap();
            if modified > latest_time {
                latest_time = modified;
                latest_file = Some(path);
            }
        }
    }

    latest_file
}

#[derive(Debug, Deserialize)]
pub struct Face {
    pub file: String,
    pub name: String
}

pub fn deserialize_from_file<T: DeserializeOwned>(path: String) -> Result<Vec<T>, serde_json::Error> {
    let file =  File::open(path).unwrap();
    let mut reader = BufReader::new(file);

    let mut buffer = String::new();
    let _ = reader.read_to_string(&mut buffer);

    serde_json::from_str(&buffer)
}
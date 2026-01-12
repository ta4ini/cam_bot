use std::path::PathBuf;

pub fn get_project_root() -> PathBuf {
    std::env::current_dir().expect("Project folder not found")
}
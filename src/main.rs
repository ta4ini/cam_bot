mod device;

use std::{
    sync::{Arc, LazyLock},
    thread,
};
use tokio::sync::RwLock;

use crate::device::{CameraInfo, camera::show_result};

pub static CAMERAS: LazyLock<Arc<RwLock<Vec<CameraInfo>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut res = device::camera::find_onvif_camera().await?;
    {
        let writer = CAMERAS.write(); // Exclusive access
        writer.await.clear();
    }

    {
        let writer = CAMERAS.write(); // Exclusive access
        writer.await.append(&mut res);
    }

    let cameras = CAMERAS.read().await;
    let mut tasks = vec![];

    for index in 0..cameras.len() {
        tasks.push(thread::spawn(async move || {
            let _ = show_result(get(index).await.as_str());
        }));
    }

    if !tasks.is_empty() {
        for task in tasks {
            let _ = task.join();
        }
    } else {
        let _ = show_result("");
    }

    Ok(())
}

async fn get(index: usize) -> String {
    let camera = CAMERAS.read().await;
    if let Some(value) = camera.get(index) {
        return value.url.clone();
    }

    "".to_owned()
}

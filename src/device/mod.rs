use std::net::IpAddr;

use opencv::{
    core::{Mat, MatTraitConst},
    videoio::{
        self, CAP_PROP_FPS, CAP_PROP_FRAME_HEIGHT, CAP_PROP_FRAME_WIDTH, VideoCapture,
        VideoCaptureTrait, VideoCaptureTraitConst as _,
    },
};

pub mod camera;
pub mod message;

#[derive(Debug, Clone)]
pub struct CameraInfo {
    pub url: String,
    pub ip_addres: IpAddr,
    pub port: u16,
    pub id: String,
}

impl CameraInfo {
    pub fn new(url: String, ip_addres: IpAddr, port: u16, id: String) -> Self {
        CameraInfo {
            url,
            ip_addres,
            port,
            id,
        }
    }
}

#[derive(Debug)]
pub struct FrameData {
    frame: Mat,
    // timestamp: std::time::Instant,
    id: String,
}

pub struct CameraSettings {
    ip_camera_url: String,
    cap: VideoCapture,
    id: String,
}

impl CameraSettings {
    pub fn new(
        ip_camera_url: String,
        id: String,
    ) -> opencv::Result<Self, Box<dyn std::error::Error + Send>> {
        println!("ip camera url {:?}", ip_camera_url);
        let cap = match ip_camera_url.is_empty() {
            true => videoio::VideoCapture::new(0, videoio::CAP_ANY),
            _ => videoio::VideoCapture::from_file(&ip_camera_url, videoio::CAP_FFMPEG),
        }
        .unwrap();

        if !cap.is_opened().unwrap() {
            panic!("Unable to open default camera!");
        }

        println!(
            "Frame width: {}",
            cap.get(CAP_PROP_FRAME_WIDTH).unwrap().round()
        );
        println!(
            "Frame height: {}",
            cap.get(CAP_PROP_FRAME_HEIGHT).unwrap().round()
        );

        let fps = cap.get(CAP_PROP_FPS).unwrap();
        println!("FPS: {}", fps);
        let delay = (1000. / fps).round();
        println!("Delay: {}", delay);

        Ok(Self {
            ip_camera_url,
            cap,
            id,
        })
    }

    pub fn get_frame(&mut self) -> opencv::Result<FrameData> {
        let mut frame = Mat::default();

        if !self.cap.read(&mut frame)? {
            panic!("No frame")
        }

        let frame_size = frame.size()?;

        if frame_size.width == 0 || frame_size.height == 0 {
            panic!("Farme id empty");
        }

        let frame_data = FrameData {
            frame,
            // timestamp: std::time::Instant::now(),
            id: self.id.to_string(),
        };

        Ok(frame_data)
    }
}

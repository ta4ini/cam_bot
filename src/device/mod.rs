use std::net::IpAddr;

pub mod camera;
pub mod message;

#[derive(Debug)]
pub struct CameraInfo {
    pub url: String,
    pub ip_addres: IpAddr,
    pub port: u16,
}

// pub struct Devices {
//     cameras_info: Vec<CameraInfo>
// }

impl CameraInfo {
    pub fn new(url: String, ip_addres: IpAddr, port: u16) -> Self {
        CameraInfo {
            url,
            ip_addres,
            port,
        }
    }
}

// impl Devices {
//     pub fn add(&mut self, camera_info: CameraInfo){
//         self.add(camera_info);
//     }
// }

use chrono::Local;
use image::{DynamicImage, RgbImage};
use log::warn;
use opencv::{
    core::{AlgorithmHint, CV_8U, Mat, Point, Rect, Scalar, Size, Vector},
    imgcodecs,
    imgproc::{
        self, COLOR_BGR2RGB, ContourApproximationModes, MORPH_CLOSE,
        MORPH_ELLIPSE, RetrievalModes, contour_area, cvt_color, find_contours,
        get_structuring_element, morphology_ex,
    },
    objdetect::{self, CASCADE_SCALE_IMAGE},
    prelude::*,
    video::create_background_subtractor_mog2,
};
use utils::{create_dir, get_motion_folder_path, get_project_root};
use std::{
    io::{BufReader, Read},
    time::Duration,
};
use tokio::{
    net::UdpSocket,
    sync::{broadcast, mpsc},
    time::timeout,
};
use uuid::Uuid;
use xml::reader::{EventReader, XmlEvent};
use yolo::find_object_by_yolo;

use crate::device::{CameraInfo, CameraSettings, FrameData};

const WS_DISCOVERY_IP_MULTICAST_ADDRESS: &str = "239.255.255.250:3702";
const UDP_SOCKET_ADDR: &str = "0.0.0.0:0";

pub async fn find_onvif_camera() -> Result<Vec<CameraInfo>, Box<dyn std::error::Error + Send + Sync>>
{
    // Bind to "0.0.0.0" by default
    // This is to receive incoming replies
    let udp_client = UdpSocket::bind(UDP_SOCKET_ADDR).await?;

    // Get the XML SOAP message to broadcast
    let uuid = Uuid::new_v4();
    let msg_discover = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
            <e:Envelope xmlns:e="http://www.w3.org/2003/05/soap-envelope"
                xmlns:w="http://schemas.xmlsoap.org/ws/2004/08/addressing"
                xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery"
                xmlns:dn="http://www.onvif.org/ver10/network/wsdl">
                <e:Header>
                    <w:MessageID>
                        uuid:{uuid}</w:MessageID>
                    <w:To>
                        urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To>
                    <w:Action>
                        http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action>
                </e:Header>
                <e:Body>
                    <d:Probe>
                        <d:Types>dn:NetworkVideoTransmitter</d:Types>
                    </d:Probe>
                </e:Body>
            </e:Envelope>"#
    );

    // Get responses to broadcast message
    let mut camera_found: Vec<CameraInfo> = Vec::new();
    let mut try_send = 0;

    while try_send < 2 {
        let mut try_recv = 0;
        try_send += 1;

        // Send the SOAP message over UDP
        // Use default IP and Port
        let success = udp_client
            .send_to(msg_discover.as_ref(), WS_DISCOVERY_IP_MULTICAST_ADDRESS)
            .await?;

        log::info!("Успех: {}", success);

        while try_recv < 5 {
            try_recv += 1;
            let mut buf = Vec::with_capacity(4096);

            // Wait 1 sec for a response
            if let Ok(recv) = timeout(
                Duration::from_millis(2000),
                udp_client.recv_buf_from(&mut buf),
            )
            .await
            {
                match recv {
                    Ok((size, addr)) => {
                        log::info!("[OnvifClient][Discover] Received response from: {addr}");

                        let buffer = BufReader::new(&buf[..size]);
                        let parser = EventReader::new(buffer);

                        for xml in parser {
                            if let Ok(XmlEvent::Characters(c)) = xml
                                && c.contains("NetworkVideoTransmitter")
                            {
                                match camera_found.iter().find(|info| info.ip_addres == addr.ip()) {
                                    Some(res) => {
                                        println!("Camera exists{:?}", res.url);
                                        break;
                                    }
                                    None => camera_found.push(CameraInfo {
                                        url: format!(
                                            "rtsp://{}:554/user=admin&password=&channel=1&stream=0.sdp",
                                            addr.ip()
                                        ),
                                        ip_addres: addr.ip(),
                                        port: addr.port(),
                                        id: Uuid::new_v4().to_string()
                                    }),
                                }
                            }
                        }
                    }
                    Err(e) => log::error!(" Error in response {e}"),
                }
            }
        }
    }

    Ok(camera_found)
}

pub async fn camera_task(
    camera_info: &CameraInfo,
    frame_sender: mpsc::Sender<FrameData>,
    mut stop_receiver: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send>> {
    println!("Camera info {:?}", camera_info);

    let mut camera = match CameraSettings::new(camera_info.url.to_string(), camera_info.id.clone())
    {
        Ok(cam) => cam,
        Err(e) => {
            log::error!(
                "Failed to initialize camera {}:, error: {}",
                camera_info.url,
                e
            );
            return Err(e);
        }
    };

    loop {
        if stop_receiver.try_recv().is_ok() {
            println!("Stop all camera tasks");
            break;
        }

        // Capture frame
        match camera.get_frame() {
            Ok(frame_data) => {
                // Send frame to processing channel
                if let Err(e) = frame_sender.send(frame_data).await {
                    warn!(
                        "Failed to send frame from: {}, error: {}",
                        camera.ip_camera_url, e
                    );
                    break;
                }
            }
            Err(e) => {
                log::error!(
                    "Error capturing frame from: {}, error: {}",
                    camera.ip_camera_url,
                    e
                );
                return Err(Box::new(e));
            }
        }

        // Small delay to prevent busy looping
        tokio::task::yield_now().await;
    }

    Ok(())
}

pub async fn use_farme(
    mut frame_receiver: mpsc::Receiver<FrameData>,
    mut stop_receiver: broadcast::Receiver<()>,
) -> opencv::Result<()> {
    let root = get_project_root();

    let mut bg_subtractor: opencv::core::Ptr<opencv::video::BackgroundSubtractorMOG2> =
        create_background_subtractor_mog2(1000, 16.0, true).unwrap(); // MOG2 subtractor

    // Create MOG2 background subtractor
    bg_subtractor.set_detect_shadows(false).unwrap(); // Faster, less noisy (set true if you need shadow detection)

    //morph operations
    let kernel = get_structuring_element(
        MORPH_ELLIPSE,
        opencv::core::Size::new(7, 7),
        opencv::core::Point::new(-1, -1),
    )?;

    let mut fg_mask = Mat::default(); // Foreground mask (motion result)
    let mut contours = Vector::<Vector<opencv::core::Point>>::new();
    let mut fg_mask_clean = Mat::default();

    loop {
        if stop_receiver.try_recv().is_ok() {
            println!("Stop all use frame tasks");
            break;
        }

        let folder_path = get_motion_folder_path();
        create_dir(&folder_path.display().to_string()).unwrap();

        let frame_data = frame_receiver.recv().await;

        match frame_data {
            Some(frame_data) => {
                let mut display = frame_data.frame.clone();

                // Apply background subtractor
                //bg_subtractor.apply(&frame_data.frame, &mut fg_mask, -1.0);
                opencv::prelude::BackgroundSubtractorMOG2Trait::apply(
                    &mut bg_subtractor,
                    &frame_data.frame,
                    &mut fg_mask,
                    0.3, //3.01 slow adaptive, artificles | 0.1 rapid | 0.5 - 0.1 good
                )
                .expect("Apply background subtractor"); // -1.0 uses default learning rate

                morphology_ex(
                    &fg_mask,
                    &mut fg_mask_clean,
                    MORPH_CLOSE,
                    &kernel,
                    opencv::core::Point::new(-1, -1),
                    1,
                    0,
                    Scalar::all(0.0),
                )
                .expect("Advanced morph transformations");

                // Find contours in the foreground mask
                find_contours(
                    &fg_mask_clean,
                    &mut contours,
                    RetrievalModes::RETR_EXTERNAL.into(), // Example retrieval mode
                    ContourApproximationModes::CHAIN_APPROX_SIMPLE.into(), // Example approximation method
                    Point::new(0, 0),                                      // Offset
                )?;

                // Draw bounding boxes around significant motion
                for contour in contours.iter() {
                    let area = contour_area(&contour, false).expect("Calculate area");
                    // let motion_pixels = count_non_zero(&fg_mask_clean).expect("Get moition pixel");
                    //println!("motion_pixels {}, area: {}", motion_pixels, area);
                    // if area > 10_000.0 && area < 50_000.0 && motion_pixels > 75_000 {
                    if area < 5_000.0 {
                        continue;
                    }

                    let bounding_boxes =
                        find_object_by_yolo(mat_to_dynamic_image(&display).unwrap());
                    // bounding_boxes Ok([BoundingBox { x1: 306.89105, y1: 271.7392, x2: 702.68005, y2: 447.72693 }])
                    // FACE Rect_ { x: 505, y: 353, width: 320, height: 320 }
                    match bounding_boxes {
                        Ok(boxes) => {
                            for body in boxes.iter() {
                                imgproc::rectangle(
                                    &mut display,
                                    Rect {x: body.x1 as i32, y: body.y1 as i32,  width: body.x2 as i32, height: body.y2 as i32 },
                                    Scalar::new(0.0, 255.0, 0.0, 0.0), // Green BGR B G R A
                                    2, // thickness
                                    imgproc::LINE_8,
                                    0,
                                )?;
                            }

                            // Draw result
                            if !boxes.is_empty() {
                                let filename = folder_path.join(format!("motion_{}_{}.jpg", Local::now().format("%Y-%m-%d %H:%M:%S"), frame_data.id)).display().to_string();
                                println!("filename {:?}", filename);
                                //save image
                                opencv::imgcodecs::imwrite(
                                    &filename,
                                    &display,
                                    &Vector::new(),
                                )?;
                            }
                        },
                        Err(err) => log::error!("Error {}", err)
                    };
                }

                //load model from Git OPENCV
                let mut face_cascade = objdetect::CascadeClassifier::new(
                    &root
                        .join("model")
                        .join("haarcascade_frontalface_default.xml")
                        .display()
                        .to_string(),
                )
                .expect("Can not load model from Git OPENCV: haarcascade_frontalface_default.xml");

                let mut gray = Mat::default();
                opencv::imgproc::cvt_color(
                    &frame_data.frame,
                    &mut gray,
                    opencv::imgproc::COLOR_BGR2GRAY,
                    0,
                    AlgorithmHint::ALGO_HINT_DEFAULT,
                )
                .expect("Can not convert original image to gray color");
                //find face
                let mut faces = Vector::<Rect>::new();
                face_cascade
                    .detect_multi_scale(
                        &gray,
                        &mut faces,
                        1.1,
                        40, //чем выше тем меньше ложных срабатываний
                        CASCADE_SCALE_IMAGE,
                        Size::new(30, 30),
                        Size::new(0, 0),
                    )
                    .expect("Can not detect difference between images: detect face");

                //draw red rectangle
                for face in faces.iter() {
                    // println!("FACE {:?}", face);
                    imgproc::rectangle(
                        &mut display,
                        face,
                        Scalar::new(0.0, 0.0, 255.0, 0.0), //B G R A
                        2,                                 // thickness
                        imgproc::LINE_8,
                        0,
                    )?;
                }

                if !faces.is_empty() {
                    log::info!("Face count: {}", faces.len());
                    
                    let filename = folder_path.join(format!("face_{}_{}.jpg", Local::now().format("%Y-%m-%d %H:%M:%S"), frame_data.id)).display().to_string();
                    imgcodecs::imwrite(&filename, &display, &Vector::new()).unwrap();
                }
            }
            None => {
                warn!("Frame channel closed");
                break;
            }
        }
    }

    Ok(())
}

pub fn mat_to_dynamic_image(mat: &Mat) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    // let channel = mat.channels(); //channel 3
    let depth = mat.depth();

    // Only support 8-bit depth
    if depth != CV_8U {
        panic!("Only 8-bit images (CV_8U) are supported");
    }

    // println!("channels {}", channels);
    let width = mat.cols();
    let height = mat.rows();

    let mut rgb_mat = Mat::default();
    cvt_color(
        mat,
        &mut rgb_mat,
        COLOR_BGR2RGB,
        3,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;
    //buffer for RGB image
    let mut buffer = vec![0u8; (width * height * 3) as usize];
    let mut byte_slice: &[u8] = rgb_mat.data_bytes().unwrap();
    byte_slice.read_exact(&mut buffer).unwrap();
    let rgb_img = RgbImage::from_raw(width as u32, height as u32, buffer).unwrap();

    Ok(DynamicImage::ImageRgb8(rgb_img))
    // match channels {
    //     1 => {
    //         // Grayscale → GrayImage
    //         let mut buffer = vec![0u8; (width * height) as usize];
    //         mat.data_bytes().read_exact(&mut buffer)?;
    //         let gray_img = GrayImage::from_raw(width as u32, height as u32, buffer)
    //             .ok_or_else(|| anyhow::anyhow!("Failed to create GrayImage"))?;
    //         Ok(DynamicImage::ImageLuma8(gray_img))
    //     }

    //     3 => {
    //         // BGR → RGB
    //         let mut rgb_mat = Mat::default();
    //         cvt_color(mat, &mut rgb_mat, COLOR_BGR2RGB, 3)?;
    //         let mut buffer = vec![0u8; (width * height * 3) as usize];
    //         rgb_mat.data_bytes().read_exact(&mut buffer)?;
    //         let rgb_img = RgbImage::from_raw(width as u32, height as u32, buffer)
    //             .ok_or_else(|| anyhow::anyhow!("Failed to create RgbImage"))?;
    //         Ok(DynamicImage::ImageRgb8(rgb_img))
    //     }

    //     4 => {
    //         // BGRA → RGBA
    //         let mut rgba_mat = Mat::default();
    //         cvt_color(mat, &mut rgba_mat, COLOR_BGR2RGBA, 4)?;
    //         let mut buffer = vec![0u8; (width * height * 4) as usize];
    //         rgba_mat.data_bytes().read_exact(&mut buffer)?;
    //         let rgba_img = RgbaImage::from_raw(width as u32, height as u32, buffer)
    //             .ok_or_else(|| anyhow::anyhow!("Failed to create RgbaImage"))?;
    //         Ok(DynamicImage::ImageRgba8(rgba_img))
    //     }

    //     _ => anyhow::bail!("Unsupported channel count: {}", channels),
    // }
}

//     use face_recognition::{FaceLandmarks, FaceRecognition};
//     https://github.com/ulagbulag/dlib-face-recognition/blob/master/examples/compare_faces/src/main.rs
//     let known_image = image::open("known.jpg").unwrap();
//     let unknown_image = image::open("unknown.jpg").unwrap();

//     let known_encodings = FaceRecognition::get_face_encodings(&known_image, None);
//     let unknown_encodings = FaceRecognition::get_face_encodings(&unknown_image, None);

//     if let (Some(known), Some(unknown)) = (known_encodings.first(), unknown_encodings.first()) {
//         let distance = FaceRecognition::face_distance(&[known.clone()], &unknown);
//         if distance[0] < 0.6 { // Threshold for match
//             println!("Match!");
//         }
//     }

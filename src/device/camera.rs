use log::warn;
use opencv::{
    core::{Mat, Point, Rect, Scalar, Size, Vector, count_non_zero},
    imgcodecs,
    imgproc::{
        self, COLOR_BGR2GRAY, ContourApproximationModes, LINE_8, MORPH_CLOSE, MORPH_ELLIPSE,
        RetrievalModes, bounding_rect, contour_area, cvt_color, find_contours,
        get_structuring_element, morphology_ex, rectangle,
    },
    objdetect::{self, CASCADE_SCALE_IMAGE},
    prelude::*,
    video::create_background_subtractor_mog2,
};
use std::{io::BufReader, path::PathBuf, time::Duration};
use tokio::{
    net::UdpSocket,
    sync::{broadcast, mpsc},
    time::timeout,
};
use uuid::Uuid;
use xml::reader::{EventReader, XmlEvent};

use crate::device::{CameraInfo, CameraSettings, FrameData};

const WS_DISCOVERY_IP_MULTICAST_ADDRESS: &str = "239.255.255.250:3702";
const UDP_SOCKET_ADDR: &str = "0.0.0.0:0"; // let OS choose port

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
                    -1.0,
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
                    let motion_pixels = count_non_zero(&fg_mask_clean).expect("Get moition pixel");
                    //println!("motion_pixels {}, area: {}", motion_pixels, area);
                    // if area > 10_000.0 && area < 50_000.0 && motion_pixels > 75_000 {
                    if area < 5_000.0 {
                        continue;
                    }
                    let mut body_cascade = objdetect::CascadeClassifier::new(
                        &root
                            .join("model")
                            .join("haarcascade_fullbody.xml")
                            .display()
                            .to_string(),
                    )
                    .expect("Can not load model from Git OPENCV: haarcascade_fullbody.xml");
                    // Ignore small noise (adjust as needed)
                    let rect = bounding_rect(&contour).expect("Calculate bounding rect");
                    rectangle(
                        &mut display,
                        rect,
                        Scalar::new(0.0, 255.0, 0.0, 0.0), // Green BGR
                        2,
                        LINE_8,
                        0,
                    )?;

                    let motion_rect = bounding_rect(&contour)?;

                    // Extract ROI (region of motion)
                    let roi = Mat::roi(&frame_data.frame, motion_rect)?;

                    // Convert ROI to grayscale (Haar requires grayscale)
                    let mut roi_gray = Mat::default();
                    cvt_color(&roi, &mut roi_gray, COLOR_BGR2GRAY, 0)?;

                    // Run Haar Cascade on ROI
                    let mut bodies = Vector::<Rect>::new();
                    body_cascade.detect_multi_scale(
                        &roi_gray,
                        &mut bodies,
                        1.1,                // scale_factor
                        3,                 // min_neighbors
                        0,                  // flags (use default)
                        Size::new(60, 120), // min_size (adjust based on your scene)
                        Size::new(0, 0),    // max_size (0 = no limit)
                    )?;

                    // Draw result
                    if !bodies.is_empty() {
                        println!("motion_pixels {}, area: {}", motion_pixels, area);

                        let filename = format!("motion-{}.jpg", frame_data.id);
                        let folder =
                            std::env::var("MOTION_FOLDER").unwrap_or_else(|_| "motion".into());
                        let path = root.join(folder).join(filename);

                        //save image
                        opencv::imgcodecs::imwrite(
                            &path.display().to_string(),
                            &display,
                            &Vector::new(),
                        )?;
                    }
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
                    let output_path = &root
                        .join("motion")
                        .join(format!("face-{}.jpg", frame_data.id))
                        .display()
                        .to_string();
                    imgcodecs::imwrite(output_path, &display, &Vector::new()).unwrap();
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

pub fn get_project_root() -> PathBuf {
    std::env::current_dir().expect("Project folder not found")
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

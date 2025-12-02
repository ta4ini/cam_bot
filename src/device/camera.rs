use chrono::{DateTime, Local};
use opencv::{
    core::{Mat, Point, Rect, Scalar, Size, Vector, count_non_zero},
    highgui::{self, wait_key},
    imgproc::{
        self, ContourApproximationModes, LINE_8, MORPH_CLOSE, MORPH_ELLIPSE, RetrievalModes,
        bounding_rect, contour_area, find_contours, get_structuring_element, morphology_ex,
        rectangle,
    },
    objdetect::{self, CASCADE_SCALE_IMAGE},
    prelude::*,
    video::create_background_subtractor_mog2,
    videoio::{self, CAP_PROP_FPS, CAP_PROP_FRAME_HEIGHT, CAP_PROP_FRAME_WIDTH},
};
use std::{io::BufReader, path::PathBuf, time::Duration};
use tokio::{net::UdpSocket, time::timeout};
use uuid::Uuid;
use xml::reader::{EventReader, XmlEvent};

use crate::device::CameraInfo;

const WS_DISCOVERY_IP_MULTICAST_ADDRESS: &str = "239.255.255.250:3702";
const UDP_SOCKET_ADDR: &str = "0.0.0.0:0"; // let OS choose port

pub async fn find_onvif_camera() -> Result<Vec<CameraInfo>, Box<dyn std::error::Error>> {
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

        println!("Try send {:?}", success);
        while try_recv < 5 {
            try_recv += 1;
            let mut buf = Vec::with_capacity(4096);

            println!("Try listen");
            // Wait 1 sec for a response
            if let Ok(recv) = timeout(
                Duration::from_millis(2000),
                udp_client.recv_buf_from(&mut buf),
            )
            .await
            {
                match recv {
                    Ok((size, addr)) => {
                        println!("[OnvifClient][Discover] Received response from: {addr}");

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
                                            "rtsp://{}:3702/user=admin&password=",
                                            addr.ip()
                                        ),
                                        ip_addres: addr.ip(),
                                        port: addr.port(),
                                    }),
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!(" Error in response {e}"),
                }
            }
        }
    }

    Ok(camera_found)
}

pub fn show_result(ip_camera_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let window = "video capture";
    highgui::named_window(window, highgui::WINDOW_AUTOSIZE)?;

    let mut cap = match ip_camera_url.is_empty() {
        true => videoio::VideoCapture::new(0, videoio::CAP_ANY)?,
        _ => videoio::VideoCapture::from_file(ip_camera_url, videoio::CAP_FFMPEG)?,
    };

    if !cap.is_opened()? {
        panic!("Unable to open default camera!");
    }

    println!("Frame width: {}", cap.get(CAP_PROP_FRAME_WIDTH)?.round());
    println!("Frame height: {}", cap.get(CAP_PROP_FRAME_HEIGHT)?.round());

    let fps = cap.get(CAP_PROP_FPS)?;
    println!("FPS: {}", fps);
    let delay = (1000. / fps).round();
    println!("Delay: {}", delay);

    let mut bg_subtractor: opencv::core::Ptr<opencv::video::BackgroundSubtractorMOG2> =
        create_background_subtractor_mog2(500, 16.0, true)?; // MOG2 subtractor

    // Create MOG2 background subtractor
    bg_subtractor.set_detect_shadows(false)?; // Faster, less noisy (set true if you need shadow detection)

    //morph operations
    let kernel = get_structuring_element(
        MORPH_ELLIPSE,
        opencv::core::Size::new(5, 5),
        opencv::core::Point::new(-1, -1),
    )?;

    let mut frame = Mat::default();
    let mut fg_mask = Mat::default(); // Foreground mask (motion result)
    let mut contours = Vector::<Vector<opencv::core::Point>>::new();
    let mut fg_mask_clean = Mat::default();

    let root = get_project_root();

    loop {
        // Retrieve the next frame from the camera or video file and store it in the `frame` variable
        cap.read(&mut frame)?;

        if frame.empty() {
            println!("No frame captured");
            break;
        }

        // Apply background subtractor
        opencv::prelude::BackgroundSubtractorMOG2Trait::apply(
            &mut bg_subtractor,
            &frame,
            &mut fg_mask,
            -1.0,
        )?; // -1.0 uses default learning rate

        morphology_ex(
            &fg_mask,
            &mut fg_mask_clean,
            MORPH_CLOSE,
            &kernel,
            opencv::core::Point::new(-1, -1),
            1,
            0,
            Scalar::all(0.0),
        )?;

        // Find contours in the foreground mask
        find_contours(
            &fg_mask_clean,
            &mut contours,
            // &mut hierarchy,
            RetrievalModes::RETR_EXTERNAL.into(), // Example retrieval mode
            ContourApproximationModes::CHAIN_APPROX_SIMPLE.into(), // Example approximation method
            Point::new(0, 0),                     // Offset
        )?;

        // Clone original frame to draw on
        let mut display = frame.clone();

        // Draw bounding boxes around significant motion
        for contour in contours.iter() {
            let area = contour_area(&contour, false)?;
            if area > 2000.0 {
                // Ignore small noise (adjust as needed)
                let rect = bounding_rect(&contour)?;
                rectangle(
                    &mut display,
                    rect,
                    Scalar::new(0.0, 255.0, 0.0, 0.0), // Green BGR
                    2,
                    LINE_8,
                    0,
                )?;
            }
        }

        let motion_pixels = count_non_zero(&fg_mask_clean)?;
        //10_000 move hand detect
        //25_000 move body
        //50_000 5 meters body move
        //75_000 standart/default
        if motion_pixels > 50_000 && motion_pixels < 100_000 {
            // e.g., more than 500 white pixels //50_000
            println!("Motion detected! Pixels: {}", motion_pixels);
            // Trigger alert, save frame, etc.
            let now: DateTime<Local> = Local::now();

            // Format the datetime into a string suitable for a filename
            // We replace colons with hyphens as colons are not allowed in filenames on some systems
            let filename_datetime = now.format("%Y-%m-%d_%H-%M-%S").to_string();
            let filename = format!("motion_{}.jpg", filename_datetime);
            let folder = std::env::var("MOTION_FOLDER").unwrap_or_else(|_| "motion".into());
            let path = root.join(folder).join(filename);
            println!("{:?}", path);

            //save image
            opencv::imgcodecs::imwrite(&path.display().to_string(), &frame, &Vector::new())?;
        }

        //DETECT FACE  use haarcascade_frontalface_default.xml
        //load model from Git OPENCV
        let mut face_cascade = objdetect::CascadeClassifier::new(
            &root
                .join("model")
                .join("haarcascade_frontalface_default.xml")
                .display()
                .to_string(),
        )?;

        let mut gray = Mat::default();
        opencv::imgproc::cvt_color(&frame, &mut gray, opencv::imgproc::COLOR_BGR2GRAY, 0)?;
        //find face
        let mut faces = Vector::<Rect>::new();
        face_cascade.detect_multi_scale(
            &gray,
            &mut faces,
            1.1,
            3,
            CASCADE_SCALE_IMAGE,
            Size::new(30, 30),
            Size::new(0, 0),
        )?;

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
            //     let output_path = &root
            //         .join("motion")
            //         .join("face.jpg")
            //         .display()
            //         .to_string();
            //     imgcodecs::imwrite(output_path, &gray, &Vector::new())?;
            //     println!("Saved output to {}", output_path);
            println!("Found {} faces", faces.len());
        }

        // Show results
        // highgui::imshow("Original", &frame)?;
        // highgui::imshow("MOG2 Motion Mask", &fg_mask)?;
        // highgui::imshow("Cleaned Motion Mask", &fg_mask_clean)?;
        // highgui::imshow("Motion Detection", &display)?;
        highgui::imshow("Face Detection/Motion Detection", &display)?;

        // If the key pressed is 27, exit the while loop
        if wait_key(10_i32)? == 27 {
            break;
        }
    }
    // Close all windows
    highgui::destroy_all_windows()?;
    Ok(())
}

fn get_project_root() -> PathBuf {
    std::env::current_dir()
        .ok()
        .expect("Project folder not found")
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

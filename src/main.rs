use crate::device::camera::get_project_root;
use crate::telegram::handler::{handle_callback, handle_message};
mod device;
mod telegram;

use crate::device::{
    CameraInfo, FrameData,
    camera::{camera_task, use_farme},
};
use crate::telegram::user::Users;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, post, web};
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, LazyLock},
};
use teloxide::prelude::Dispatcher;
use teloxide::{
    Bot,
    dispatching::UpdateFilterExt,
    dptree,
    types::{CallbackQuery, Message, Update},
};
use tokio::sync::mpsc;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

pub static CAMERAS: LazyLock<Arc<RwLock<Vec<CameraInfo>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

pub static STOP_SENDER: LazyLock<broadcast::Sender<()>> = LazyLock::new(|| {
    let (stop_sender, _) = broadcast::channel::<()>(1);
    stop_sender
});

type ArcRwLockUsers = Arc<RwLock<Users>>;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    pretty_env_logger::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8282".into());
    let addr = format!("{}:{}", host, port);

    let bot_token = std::env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN must be set");
    let bot = Bot::new(bot_token);

    let path = get_project_root()
        .join("files")
        .join("user.json")
        .display()
        .to_string();
    let users: ArcRwLockUsers = Arc::new(RwLock::new(Users::new(path)));

    // Set up the dispatcher schema
    let users_message = users.clone();
    let users_callback_query = users.clone();
    let schema = dptree::entry()
        .branch(
            Update::filter_message().endpoint(move |bot: Bot, msg: Message| {
                let users: ArcRwLockUsers = users_message.clone();
                async move { handle_message(bot, msg, users).await }
            }),
        )
        .branch(
            Update::filter_callback_query().endpoint(move |bot: Bot, q: CallbackQuery| {
                let users: ArcRwLockUsers = users_callback_query.clone();
                async move { handle_callback(bot, q, users).await }
            }),
        );

    // Start the dispatcher in a separate task
    tokio::spawn(async move {
        Dispatcher::builder(bot.clone(), schema)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    });

    HttpServer::new(|| App::new().service(clients_callback))
        .bind(addr)?
        .run()
        .await
}

//TODO: Use later
async fn get_camera_info(index: usize) -> String {
    let camera = CAMERAS.read().await;
    if let Some(value) = camera.get(index) {
        println!("{:?}", value);
        return value.url.clone();
    }

    "".to_owned()
}

//TODO: Create for clients
#[post("/clients/callback")]
pub async fn clients_callback(req: HttpRequest, body: web::Bytes) -> impl Responder {
    HttpResponse::Ok().body("ok")
}

pub async fn start_worker() -> Result<(), Box<dyn std::error::Error + Send>> {
    let mut res: Vec<CameraInfo> = {
        let cameras = CAMERAS.read().await;
        cameras.clone()
    };

    //ONLY FOR DEVELOPMENT AND DEBUG
    if res.is_empty() {
        println!("CAMERAS ID EMPTY {:?}", res);
        //web cam local
        res.push(CameraInfo {
            url: "".to_string(),
            ip_addres: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 1,
            id: Uuid::new_v4().to_string(),
        });

        let writer = CAMERAS.write(); // Exclusive access
        writer.await.append(&mut res);
    }

    //let (stop_sender, _) = broadcast::channel::<()>(1);
    // let stop_sender_clone = STOP_SENDER.clone();// stop_sender.clone();

    let (frame_sender, frame_receiver) = mpsc::channel::<FrameData>(100);
    let mut camera_handles = Vec::new();

    let camera_info_items: Vec<CameraInfo> = {
        let cameras = CAMERAS.read().await;
        cameras.clone()
    };

    for camera_info in camera_info_items {
        let frame_sender = frame_sender.clone();
        let stop_receiver = STOP_SENDER.subscribe();

        let handle =
            tokio::spawn(
                async move { camera_task(&camera_info, frame_sender, stop_receiver).await },
            );

        camera_handles.push(handle);
    }

    let frame_handle = tokio::spawn(async move {
        let stop_receiver = STOP_SENDER.subscribe();
        use_farme(frame_receiver, stop_receiver).await
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Received Ctrl+C, shutting down...");
            if let Err(e) = STOP_SENDER.send(()){
                log::error!("Stop sender: {}",e);
            }
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(360)) => {
            println!("Test duration complete, shutting down...");
            if let Err(e) = STOP_SENDER.send(()){
                log::error!("Stop sender: {}",e);
            }
        }
    }

    for handle in camera_handles {
        let _ = handle.await;
    }

    let _ = frame_handle.await;

    Ok(())
}

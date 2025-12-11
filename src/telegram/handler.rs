use crate::{
    CAMERAS, STOP_SENDER,
    device::{
        CameraInfo,
        camera::{find_onvif_camera, get_project_root},
        message::Messages,
    },
    start_worker,
    telegram::user::Users,
};
use std::ops::Index;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, Message, ParseMode},
};
use tokio::fs::{self};

pub async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    let chat_id = msg.chat.id;

    let messages = Messages::default();

    if let Some(text) = msg.text() {
        if text == "/start" {
            let path = get_project_root()
                .join("files")
                .join("user.json")
                .display()
                .to_string();
            let users = Users::new(path);

            let username = match msg.chat.username() {
                Some(s) => s.to_string(),
                None => String::from("no name"),
            };

            users
                .read_from_file()
                .await
                .add_id(chat_id.0, username)
                .write_to_file()
                .await;
            // if let Err(e) = write_to_file(msg).await {
            //     log::error!("Can't write user to file: {}", e);
            // }

            bot.send_message(chat_id, &messages.welcome).await?;

            return Ok(());
        }

        if text == "/help" {
            bot.send_message(chat_id, &messages.help_detailed)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;

            return Ok(());
        }

        if text == "/find" {
            if !is_active(chat_id.0).await {
                bot.send_message(chat_id, &messages.access_denied).await?;
                return Ok(());
            }

            bot.send_message(chat_id, &messages.search).await?;

            match find_onvif_camera().await {
                Ok(devices) => {
                    let text = if !devices.is_empty() {
                        format!("{}: {} ед.", &messages.is_device, devices.len())
                    } else {
                        "Камер нет, пробуем подключиться к локальной".to_string()
                    };

                    bot.send_message(chat_id, text).await?;

                    tokio::spawn(start_worker());
                }
                Err(e) => {
                    log::error!("Error when find devices: {}", e);
                    bot.send_message(chat_id, &messages.error).await?;
                }
            }

            return Ok(());
        }

        if text == "/stop" {
            if !is_active(chat_id.0).await {
                bot.send_message(chat_id, &messages.access_denied).await?;
                return Ok(());
            }

            if let Err(e) = STOP_SENDER.send(()) {
                log::error!("Stop sender: {}", e);
            }

            bot.send_message(chat_id, &messages.stop).await?;

            return Ok(());
        }

        if text == "/status" {
            if !is_active(chat_id.0).await {
                bot.send_message(chat_id, &messages.access_denied).await?;
                return Ok(());
            }

            let camera_info_items: Vec<CameraInfo> = {
                let cameras = CAMERAS.read().await;
                cameras.clone()
            };

            if camera_info_items.is_empty() {
                bot.send_message(chat_id, &messages.no_device).await?;

                return Ok(());
            }

            bot.send_message(
                chat_id,
                format!(
                    "{}: {}",
                    &messages.status,
                    camera_info_items
                        .iter()
                        .map(|f| { format!("{}:{}", f.ip_addres.clone(), f.port.clone()) })
                        .collect::<Vec<String>>()
                        .join(", ")
                ),
            )
            .await?;

            if camera_info_items.is_empty() {
                return Ok(());
            }

            let mut inline_keyboard = vec![];
            let mut index = 0;
            for chunks_info in camera_info_items.chunks(2) {
                let mut chunks = vec![];
                for value in chunks_info {
                    chunks.push(InlineKeyboardButton::callback(
                        format!("Камера {}", value.id),
                        format!("camera_{}", index),
                    ));
                    index += 1;
                }
                inline_keyboard.push(chunks);
            }
            bot.send_message(chat_id, "Выбрать камеру и загрузить изображение:")
                .reply_markup(InlineKeyboardMarkup::new(inline_keyboard))
                .await?;

            return Ok(());
        }

        if text == "/access" {
            let path = get_project_root()
                .join("files")
                .join("user.json")
                .display()
                .to_string();
            let users = Users::new(path);
            let users = users.clone().read_from_file().await;
            let (is_active, is_admin) = users.is_access(chat_id.0);

            if is_active && is_admin {
                let mut buttons = vec![];
                for u in users.all_users() {
                    if u.is_admin {
                        continue;
                    }
                    buttons.push(InlineKeyboardButton::callback(
                        format!("{} {} {}", u.chat_id, u.username, u.is_active),
                        format!("user_{}|{}", u.chat_id, u.is_active),
                    ));
                }

                bot.send_message(chat_id, "Участники:")
                    .reply_markup(InlineKeyboardMarkup::new(vec![buttons]))
                    .await?;
            }

            return Ok(());
        }
    }

    bot.send_message(chat_id, &messages.unknown).await?;

    Ok(())
}

pub async fn handle_callback(bot: Bot, q: CallbackQuery) -> ResponseResult<()> {
    // println!("{:?}", q);

    if let Some(data) = q.data.as_deref() {
        let messages = Messages::default();

        if data.contains("camera_") {
            let info: Vec<&str> = data.split('_').collect();
            match info[1].parse::<usize>() {
                Ok(index) => {
                    let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));

                    let camera_info_items: Vec<CameraInfo> = {
                        let cameras = CAMERAS.read().await;
                        cameras.clone()
                    };

                    if camera_info_items.is_empty() {
                        bot.send_message(chat_id, &messages.no_device).await?;

                        return Ok(());
                    }

                    for prefix in ["face", "motion"] {
                        let path = get_project_root()
                            .join("motion")
                            .join(format!(
                                "{}-{}.jpg",
                                prefix,
                                camera_info_items.index(index).id
                            ))
                            .display()
                            .to_string();

                        match fs::metadata(&path).await {
                            Ok(metadata) => {
                                if metadata.is_file() {
                                    bot.send_photo(chat_id, InputFile::file(path))
                                        .caption(format!(
                                            "Изображение с камеры: {}",
                                            camera_info_items.index(index).id
                                        ))
                                        .await?;
                                } else {
                                    bot.send_message(chat_id, "Изображений нет").await?;
                                }
                            }
                            _ => continue,
                        }
                    }
                }
                Err(e) => log::error!("Error parsing '{}': {}", info[1], e),
            }
        }
    }

    Ok(())
}

async fn is_active(chat_id: i64) -> bool {
    let path = get_project_root()
        .join("files")
        .join("user.json")
        .display()
        .to_string();
    let users = Users::new(path);
    let (is_active, _) = users.read_from_file().await.is_access(chat_id);
    is_active
}

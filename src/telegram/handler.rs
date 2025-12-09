use std::{fs, ops::Index};

// use crate::locales::messages::Messages;
use crate::{
    CAMERAS, STOP_SENDER,
    device::{
        CameraInfo,
        camera::{find_onvif_camera, get_project_root},
        message::Messages,
    },
    start_worker,
};
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, Message, ParseMode},
};

pub async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let messages = Messages::default();
    // println!("{:?}", msg);
    if let Some(text) = msg.text() {
        if text == "/start" {
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
            println!("{}", text);
            let _ = STOP_SENDER.send(());

            bot.send_message(chat_id, &messages.stop).await?;

            return Ok(());
        }

        if text == "/status" {
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

                        if fs::metadata(&path).is_err() {
                            continue;
                        }

                        bot.send_photo(chat_id, InputFile::file(path))
                            .caption(format!(
                                "Изображение с камеры: {}",
                                camera_info_items.index(index).id
                            ))
                            .await?;
                    }
                }
                Err(e) => log::error!("Error parsing '{}': {}", info[1], e),
            }
        }
    }

    Ok(())
}

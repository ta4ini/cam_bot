// use crate::locales::messages::Messages;
use crate::{
    CAMERAS, STOP_SENDER, device::{CameraInfo, camera::find_onvif_camera, message::Messages}, start_worker
};
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, Message, ParseMode},
};

pub async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let messages = Messages::default();

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
            match find_onvif_camera().await {
                Ok(devices) => {
                    bot.send_message(
                        chat_id,
                        format!("{}: {} ед.", &messages.is_device, devices.len()),
                    )
                    .await?;

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

            return Ok(());
        }
    }

    bot.send_message(chat_id, &messages.unknown).await?;

    Ok(())
}

pub async fn handle_callback(bot: Bot, q: CallbackQuery) -> ResponseResult<()> {
    println!("{:?}", q);

    // if let Some(data) = q.data.as_deref() {
    //     let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));

    //     bot.send_message(chat_id, greeting).await?;
    // }

    Ok(())
}

use crate::db::queries;
// use crate::locales::messages::Messages;
use crate::services::chart::draw_weekly_calories_chart;
use chrono::Utc;
use log::error;
use reqwest::Url;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, Message, ParseMode},
};
use crate::message::Messages;

pub async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let messages = Messages::Default();
    let camera_info = Vec<CameraInfo>::new();

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
            //match find_onvif_camera().await {
        //         Ok(devices) => {
        //              camera_info = devices;
        //             bot.send_message(chat_id, &messages.is_device).await?;
        //         }
        //         Err(e) => {
        //             log::error!("Error when find devices: {}", e.to_string());
        //             bot.send_message(chat_id, &messages.error).await?;
        //         }
        //     }

            return Ok(());
        }

        if text == "/sensitivity" {
            //чувствительность

            return Ok(());
        }

        if text == "/status" {
            if camera_info.len() == 0{
                bot.send_message(chat_id,&messages.no_device,).await?;

                return Ok(());
            }

            bot.send_message(
                chat_id,
                &messages.status,
            ).await?;
            

            return Ok(());
        }

        return Ok(());
    }

    if let Some(photos) = msg.photo() {
        if let Some(photo) = photos.last() {
            let file_id = &photo.file.id;
            let file = bot.get_file(file_id).send().await?;
            let token = std::env::var("TELEGRAM_BOT_TOKEN").unwrap();
            let url = format!("https://api.telegram.org/file/bot{}/{}", token, file.path);

            match crate::services::nutrition::analyze_image(&url, &user_lang).await {
                Ok((summary, suggestion)) => {
                    
                    //bot.send_message(chat_id, response).await?;
                }
                Err(e) => {
                    log::error!("Error in analyze_image: {}", e);
                    bot.send_message(chat_id, &messages.unknown).await?;
                }
            }
        }
        return Ok(());
    }

    bot.send_message(chat_id, &messages.unknown).await?;
    Ok(())
}

pub async fn handle_callback(bot: Bot, q: CallbackQuery) -> ResponseResult<()> {
    // if let Some(data) = q.data.as_deref() {
    //     let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));

    //     bot.send_message(chat_id, greeting).await?;
    // }

    Ok(())
}
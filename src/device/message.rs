pub struct Messages {
    pub welcome: String,
    pub help: String,
    pub help_detailed: String,
    pub unknown: String,
    pub no_device: String,
    pub error: String,
    pub status: String,
    pub is_device: String,
    pub stop_record: String
}

impl Default for Messages {
    fn default() -> Self {
        Messages {
            welcome: "Добро пожаловать!".into(),
            help: "Помощь.".into(),
            help_detailed: r#"
*Команды:*
• `/start` Запуск\.
• `/find` Найти устройства, если есть новые\.
• `/help` Помощь\.
• `/status` Сатус камер, получить изображение\.
• `/stop_record` Остановить обработку изображений\.
"#
            .to_string(),
            unknown: "Я не понял команду.".into(),
            no_device: "Нет устройств (камер).".into(),
            is_device: "Устройства обнаружены (камеры)".into(),
            error: "Ошибка.".into(),
            stop_record: "Камеры прекратили запись".into(),
            status: "Статус камер.".into(),
        }
    }
}

pub struct Messages {
    pub welcome: String,
    // pub help: String,
    pub help_detailed: String,
    pub unknown: String,
    pub no_device: String,
    pub error: String,
    pub status: String,
    pub is_device: String,
    pub stop: String,
    pub search: String,
    pub access_denied: String,
}

impl Default for Messages {
    fn default() -> Self {
        Messages {
            welcome: "Добро пожаловать!".into(),
            // help: "Помощь.".into(),
            help_detailed: r#"
*Команды:*
• `/start` Запуск\.
• `/find` Найти устройства и запустить обработку изображений\.
• `/help` Помощь\.
• `/status` Кмеры активны\, получить изображение\.
• `/stop` Остановить обработку изображений\.
"#
            .to_string(),
            unknown: "Я не понял команду.".into(),
            no_device: "Нет устройств (камер).".into(),
            is_device: "Камеры".into(),
            error: "Ошибка.".into(),
            stop: "Камеры прекратили запись".into(),
            status: "Статус камер.".into(),
            search: "Поиск камер...".into(),
            access_denied: "Доступ запрещен".into(),
        }
    }
}

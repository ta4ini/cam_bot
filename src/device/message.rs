pub struct Messages {
    pub welcome: String,
    pub help: String,
    pub help_detailed: String,
    pub unknown: String,
    pub no_device: String,
    pub error: String,
    pub status: String,
    pub is_device: String,
}

impl Default for Messages {
    fn default() -> Self {
        Messages {
            welcome: "Добро пожаловать!".into(),
            help: "Помощь.".into(),
            help_detailed: r#"
*Команды:
• `/start` Запуск\.p
• `/find` Найти устройства, если есть новые\.
• `/help` Помощь\.
• `/status` Сатус камер, получить изображение\.
"#
            .to_string(),
            unknown: "Я не понял команду.".into(),
            no_device: "Нет устройств (камер).".into(),
            is_device: "Устройства обнаружены (камеры)".into(),
            error: "Ошибка.".into(),
            status: "Статус камер.".into(),
        }
    }
}

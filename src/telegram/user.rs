use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::BufReader;
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub chat_id: i64,
    pub date: DateTime<Utc>,
    pub username: String,
    pub is_admin: bool,
    pub is_active: bool,
}

impl User {
    fn new(chat_id: i64, username: String) -> Self {
        User {
            chat_id,
            date: Utc::now(),
            username,
            is_admin: false,
            is_active: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Users {
    pub users: Vec<User>,
    pub path: String,
}

impl Users {
    pub fn new(path: String) -> Self {
        Users {
            users: Vec::new(),
            path,
        }
    }

    pub fn push(&mut self, chat_id: i64, username: String) {
        self.users.push(User::new(chat_id, username));
    }

    pub fn activate_deactivate(&mut self, chat_id: i64) {
        if !self.is_empty() {
            for user in self.users.iter_mut() {
                if user.chat_id == chat_id {
                    user.is_active = !user.is_active;
                    break;
                }
            }
        }
    }

    pub fn remove(&mut self, chat_id: i64) {
        if !self.is_empty() {
            self.users.retain(|u| u.chat_id != chat_id)
        }
    }

    fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    pub fn all_users(&self) -> &Vec<User> {
        &self.users
    }

    pub fn is_access(&self, chat_id: i64) -> (bool, bool) {
        match &self.users.iter().find(|u| u.chat_id == chat_id) {
            Some(u) => (u.is_active, u.is_admin),
            None => (false, false),
        }
    }

    pub async fn read_from_file(&mut self) {
        if !Path::new(&self.path).exists() {
            let _ = File::create(&self.path).await;
        }

        let file = File::open(&self.path).await.unwrap();
        let mut reader = BufReader::new(file);

        let mut buffer = String::new();
        let _ = reader.read_to_string(&mut buffer).await;

        if !buffer.is_empty() {
            let users: Result<Vec<User>, serde_json::Error> = serde_json::from_str(&buffer);

            match users {
                Ok(u) => self.users = u,
                Err(e) => log::error!("Failed to deserialize JSON: {}", e),
            }
            // self.push(serde_json::from_str(&buffer).expect("Error reading user.json");
        }
    }

    pub fn add_id(&mut self, chat_id: i64, username: String) {
        if !self.users.iter().any(|u| u.chat_id == chat_id) {
            self.push(chat_id, username);
        }
    }

    pub async fn write_to_file(&self) {
        let json_string = serde_json::to_string(&self.users).unwrap();
        let _ = fs::write(self.path.clone(), json_string.as_bytes()).await;
    }
}

use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type CommandHandler =
    Arc<dyn Fn(Arc<RwLock<AppState>>, Vec<String>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Vec<u8>>> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct Command {
    pub name: String,
    pub names: Vec<String>,
    pub docs: Option<String>,
    pub confirm_with_user: bool,
    pub func: CommandHandler,
    pub args: Vec<String>,
}

impl Command {
    pub fn new(name: String, names: Vec<String>, docs: Option<String>, confirm_with_user: bool, func: CommandHandler) -> Self {
        let mut all_names = names;
        if !all_names.contains(&name) {
            all_names.push(name.clone());
        }
        Self { name, names: all_names, docs, confirm_with_user, func, args: vec![] }
    }
}

use std::collections::HashMap;
use crate::relay::ids::AppId;
use crate::relay::rooms::Rooms;

pub struct App {
    pub id: AppId,
    pub token: String,
    pub rooms: Rooms,
}

impl App {
    pub fn new(id: AppId, token: String) -> Self {
        Self {
            id,
            token,
            rooms: Rooms::new(),
        }
    }
}

#[derive(Default)]
pub struct Apps {
    by_id: HashMap<AppId, App>,
    token_to_id: HashMap<String, AppId>,
    next_id: u64,
}

impl Apps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, token: String) -> AppId {
        let app_id = AppId::new(self.next_id);
        self.next_id += 1;

        let app = App::new(app_id, token.clone());
        self.by_id.insert(app_id, app);
        self.token_to_id.insert(token, app_id);

        app_id
    }

    pub fn iter(&self) -> impl Iterator<Item = &App> {
        self.by_id.values()
    }

    pub fn get_mut(&mut self, id: AppId) -> Option<&mut App> {
        self.by_id.get_mut(&id)
    }

    pub fn get_by_token(&self, token: &str) -> Option<&App> {
        let id = self.token_to_id.get(token)?;
        self.by_id.get(id)
    }
}

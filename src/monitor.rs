use serenity::model::id::ChannelId;
use serenity::prelude::RwLock;
use std::sync::Arc;
use serenity::prelude::TypeMap;

use crate::Coordinates;

async fn monitor(client_data: Arc<RwLock<TypeMap>>, channel_id: ChannelId, coords: Coordinates) {
    // TODO: Implement tokio spawn of one particular task
    // Check atomic int/flag for active and loop
    // First retrieve the credentials
}
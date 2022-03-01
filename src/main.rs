mod commands;
mod tgtg;
mod monitor;

use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};

use commands::*;
use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::{standard::macros::group, StandardFramework},
    http::Http,
    model::{
        event::ResumedEvent,
        gateway::Ready,
        id::{ChannelId, MessageId},
    },
    prelude::*,
};
use tracing::{error, info};

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

pub struct TGTGLocationContainer;

impl TypeMapKey for TGTGLocationContainer {
    type Value = Arc<RwLock<HashMap<ChannelId, CoordinatesWithRadius>>>;
}

#[derive(Clone, Copy)]
pub struct ItemMessage {
    pub message_id: MessageId,
    pub quantity: usize,
}

pub struct TGTGItemContainer;

impl TypeMapKey for TGTGItemContainer {
    type Value = Arc<RwLock<HashMap<String, ItemMessage>>>;
}
#[derive(Copy, Clone)]
pub struct CoordinatesWithRadius {
    latitude: f64,
    longitude: f64,
    radius: u64,
}
#[derive(Debug)]
pub struct TGTGCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
}

pub struct TGTGCredentialsContainer;

impl TypeMapKey for TGTGCredentialsContainer {
    type Value = Arc<TGTGCredentials>;
}

pub struct TGTGActiveChannelsContainer;

impl TypeMapKey for TGTGActiveChannelsContainer {
    type Value = Arc<RwLock<HashSet<ChannelId>>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[group]
#[commands(ping, location, radius, status, start, stop, quit)]
struct General;

#[tokio::main]
async fn main() {
    // Load .env variables if it exists.
    dotenv::dotenv().ok();

    // Initialize the logger to use environment variables.
    tracing_subscriber::fmt::init();

    let discord_token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let tgtg_access_token =
        env::var("TGTG_ACCESS_TOKEN").expect("Expected a token in the environment");
    let tgtg_refresh_token =
        env::var("TGTG_REFRESH_TOKEN").expect("Expected a token in the environment");
    let tgtg_user_token = env::var("TGTG_USER_ID").expect("Expected a token in the environment");
    let tgtg_credentials = Arc::new(TGTGCredentials {
        access_token: tgtg_access_token,
        refresh_token: tgtg_refresh_token,
        user_id: tgtg_user_token,
    });

    let http = Http::new_with_token(&discord_token);

    // We will fetch your bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // Create the framework
    let framework = StandardFramework::new()
        .configure(|c| c.owners(owners).prefix("tg!"))
        .group(&GENERAL_GROUP);

    let mut client = Client::builder(&discord_token)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
        data.insert::<TGTGLocationContainer>(Arc::new(RwLock::new(HashMap::new())));
        data.insert::<TGTGItemContainer>(Arc::new(RwLock::new(HashMap::new())));
        data.insert::<TGTGCredentialsContainer>(tgtg_credentials.clone());
        data.insert::<TGTGActiveChannelsContainer>(Arc::new(RwLock::new(HashSet::new())));
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    tgtg::test_python().expect("python environment error");

    // Loop inform all channels
    // let client_data = client.data.clone();
    // let http = client.cache_and_http.http.clone();
    // crate::monitor::monitor_locations(tgtg_credentials, client_data, http).await;

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}

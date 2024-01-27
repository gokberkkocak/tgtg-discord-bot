mod commands;
mod db;
mod monitor;
mod tgtg;

static DEFAULT_RADIUS: u8 = 1;
static OSM_ZOOM_LEVEL: u8 = 15;

use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
    time::Duration,
};

use commands::*;
use pyo3::PyObject;
use regex::Regex;
use serenity::{
    async_trait,
    framework::{
        standard::{macros::group, Configuration},
        StandardFramework,
    },
    gateway::ShardManager,
    http::Http,
    model::{
        event::ResumedEvent,
        gateway::Ready,
        id::{ChannelId, MessageId},
    },
    prelude::*,
};
use tracing::{error, info};

static RADIUS_UNIT: &str = "km";

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<ShardManager>;
}

pub struct TGTGConfigContainer;

impl TypeMapKey for TGTGConfigContainer {
    type Value = Arc<RwLock<HashMap<ChannelId, TGTGConfig>>>;
}

#[derive(Clone, Copy)]
pub struct ItemMessage {
    pub message_id: MessageId,
    pub quantity: usize,
}

pub struct TGTGItemMessageContainer;

impl TypeMapKey for TGTGItemMessageContainer {
    type Value = Arc<RwLock<HashMap<String, ItemMessage>>>;
}
#[derive(Clone)]
pub struct TGTGConfig {
    latitude: f64,
    longitude: f64,
    radius: u8,
    regex: Option<Regex>,
}

impl TGTGConfig {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
            radius: DEFAULT_RADIUS,
            regex: None,
        }
    }

    pub fn new_with_radius(latitude: f64, longitude: f64, radius: u8) -> Self {
        Self {
            latitude,
            longitude,
            radius,
            regex: None,
        }
    }
}

#[derive(Debug)]
pub struct TGTGBindings {
    pub client: PyObject,
    pub fetch_func: PyObject,
}

pub struct TGTGBindingsContainer;

impl TypeMapKey for TGTGBindingsContainer {
    type Value = Arc<TGTGBindings>;
}

pub struct TGTGActiveChannelsContainer;

impl TypeMapKey for TGTGActiveChannelsContainer {
    type Value = Arc<RwLock<HashSet<ChannelId>>>;
}

pub struct BotDBContainer;

impl TypeMapKey for BotDBContainer {
    type Value = Arc<db::BotDB>;
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
#[commands(ping, location, radius, regex, status, start, stop, quit)]
struct General;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env variables if it exists.
    dotenvy::dotenv().ok();

    // Initialize the logger to use environment variables.
    tracing_subscriber::fmt::init();

    tgtg::check_python()?;

    let discord_token = env::var("DISCORD_TOKEN")?;
    let tgtg_access_token = env::var("TGTG_ACCESS_TOKEN")?;
    let tgtg_refresh_token = env::var("TGTG_REFRESH_TOKEN")?;
    let tgtg_user_id = env::var("TGTG_USER_ID")?;
    let tgtg_cookie = env::var("TGTG_COOKIE")?;
    let db_url = env::var("DATABASE_URL")?;
    let tgtg_credentials = Arc::new(TGTGBindings {
        client: crate::tgtg::init_client(
            &tgtg_access_token,
            &tgtg_refresh_token,
            &tgtg_user_id,
            &tgtg_cookie,
        )?,
        fetch_func: crate::tgtg::init_fetch_func()?,
    });

    let http = Http::new(&discord_token);

    // We will fetch your bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            if let Some(team) = info.team {
                owners.insert(team.owner_user_id);
            } else if let Some(owner) = &info.owner {
                owners.insert(owner.id);
            }
            match http.get_current_user().await {
                Ok(bot_id) => (owners, bot_id.id),
                Err(why) => {
                    anyhow::bail!("Could not access the bot id: {:?}", why)
                }
            }
        }
        Err(why) => {
            anyhow::bail!("Could not access application info: {:?}", why)
        }
    };

    // Bot DB
    let bot_db = Arc::new(db::BotDB::new(&db_url).await?);
    let (location_map, active_set) = bot_db.get_locations().await?;

    // Create the framework
    let framework = StandardFramework::new().group(&GENERAL_GROUP);

    framework.configure(Configuration::new().prefix("tg!").owners(owners));

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&discord_token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await?;

    let location_map_rw = Arc::new(RwLock::new(location_map));
    let active_set_rw = Arc::new(RwLock::new(active_set));
    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
        data.insert::<TGTGConfigContainer>(location_map_rw.clone());
        data.insert::<TGTGActiveChannelsContainer>(active_set_rw.clone());
        data.insert::<TGTGItemMessageContainer>(Arc::new(RwLock::new(HashMap::new())));
        data.insert::<TGTGBindingsContainer>(tgtg_credentials.clone());
        data.insert::<BotDBContainer>(bot_db);
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.shutdown_all().await;
    });

    // Start monitoring task already on db
    let data = client.data.clone();
    let http = client.http.clone();
    tokio::spawn(async move {
        // wait 10 secs first to let the bot connect to discord
        tokio::time::sleep(Duration::from_secs(10)).await;
        for (channel_id, _config) in location_map_rw.read().await.iter() {
            if active_set_rw.read().await.contains(channel_id) {
                crate::monitor::monitor_location(data.clone(), http.clone(), *channel_id).await;
                info!("Channel {}: Monitor starting (DB) ", channel_id)
            }
        }
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
    Ok(())
}

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use poise::serenity_prelude as serenity;
use pyo3::PyObject;
use regex::Regex;

use serde::Deserialize;
use serenity::all::{ChannelId, MessageId};
use tokio::sync::RwLock;

use crate::monitor::ChannelMonitor;

pub static RADIUS_UNIT: &str = "km";
pub static DEFAULT_RADIUS: u8 = 1;
pub static OSM_ZOOM_LEVEL: u8 = 15;

#[derive(Clone, Copy)]
pub struct ItemMessage {
    pub message_id: MessageId,
    pub quantity: usize,
}

#[derive(Clone)]
pub struct TGTGConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub radius: u8,
    pub regex: Option<Regex>,
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

    pub fn new_full(latitude: f64, longitude: f64, radius: u8, regex: Regex) -> Self {
        Self {
            latitude,
            longitude,
            radius,
            regex: Some(regex),
        }
    }
}

#[derive(Debug)]
pub struct TGTGBindings {
    pub client: PyObject,
    pub fetch_func: PyObject,
}

#[allow(dead_code)]
pub struct DiscordData {
    pub bot_db: Arc<crate::db::BotDB>,
    pub active_channels: Arc<RwLock<HashSet<ChannelMonitor>>>,
    pub tgtg_bindings: Arc<TGTGBindings>,
    pub tgtg_configs: Arc<RwLock<HashMap<ChannelId, TGTGConfig>>>,
}

#[derive(Debug, Deserialize)]
pub struct TGTGListing {
    pub item: Item,
    pub store: Store,
    pub display_name: String,
    pub items_available: usize,
    pub distance: f64,
    pub pickup_location: PickupLocation,
    pub pickup_interval: Option<PickupInterval>,
    pub purchase_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub item_id: String,
    pub price_including_taxes: ItemPrice,
}
#[derive(Debug, Deserialize)]
pub struct ItemPrice {
    pub code: String,
    pub minor_units: u32,
    pub decimals: u32,
}

#[derive(Debug, Deserialize)]
pub struct Store {
    pub store_name: String,
    pub logo_picture: Logo,
    pub store_time_zone: Tz,
}
#[derive(Debug, Deserialize)]
pub struct Logo {
    pub current_url: String,
}

#[derive(Debug, Deserialize)]
pub struct PickupInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PickupLocation {
    pub location: Location,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

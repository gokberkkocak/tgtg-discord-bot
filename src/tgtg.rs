use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyTuple};
use serde::Deserialize;
use tracing::info;

use crate::{CoordinatesWithRadius, TGTGCredentials};

pub(crate) fn check_python() -> PyResult<()> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let version: String = sys.getattr("version")?.extract()?;
        let locals = [("os", py.import("os")?)].into_py_dict(py);
        let code = "os.getenv('USER') or os.getenv('USERNAME') or 'Unknown'";
        let user: String = py.eval(code, None, Some(locals))?.extract()?;
        info!(
            "Python status OK! Running user: {}, Python version: {}",
            user, version
        );
        Ok(())
    })
}

fn py_get_items(
    tgtg_credentials: &TGTGCredentials,
    coords: &CoordinatesWithRadius,
) -> PyResult<String> {
    Python::with_gil(|py| {
        let fun: Py<PyAny> = PyModule::from_code(
            py,
            "from tgtg import TgtgClient
import json
def fetch_items(access_token, refresh_token, user_id, cookie, latitude, longitude, radius):
    client = TgtgClient(access_token=access_token, refresh_token=refresh_token, user_id=user_id, cookie=cookie)
    items = client.get_items(
        favorites_only=False,
        latitude=latitude,
        longitude=longitude,
        page_size=100,
        radius=radius,
    )
    return json.dumps(items)",
            "",
            "",
        )?
        .getattr("fetch_items")?
        .into();

        // call object with PyTuple
        let args = PyTuple::new(
            py,
            &[
                &tgtg_credentials.access_token,
                &tgtg_credentials.refresh_token,
                &tgtg_credentials.user_id,
                &tgtg_credentials.cookie,
                &format!("{:.5}", coords.latitude),
                &format!("{:.5}", coords.longitude),
                &format!("{}", coords.radius),
            ],
        );
        let ret = fun.call1(py, args)?;
        let items = ret.extract::<String>(py)?;
        Ok(items)
    })
}

pub fn get_items(
    tgtg_credentials: &TGTGCredentials,
    coords: &CoordinatesWithRadius,
) -> anyhow::Result<Vec<TGTGListing>> {
    let py_items = py_get_items(tgtg_credentials, coords)?;
    let items: Vec<TGTGListing> = serde_json::from_str(&py_items)?;
    Ok(items)
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


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_python() -> PyResult<()> {
        check_python()
    }

    #[test]
    fn test_tgtg_module() -> PyResult<()> {
        Python::with_gil(|py| {
            py.import("tgtg")?;
            Ok(())
        })
    }
}

use pyo3::ffi::c_str;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyString, PyTuple};
use tracing::info;

use anyhow::Context as _;

use crate::data::{BrowseOrigin, ItemFetchResult, TGTGBindings, TGTGListing};

pub(crate) fn check_python() -> PyResult<()> {
    Python::attach(|py| {
        let sys = py.import("sys")?;
        let version: String = sys.getattr("version")?.extract()?;
        let locals = [("os", py.import("os")?)].into_py_dict(py)?;
        let code = c_str!("os.getenv('USER') or os.getenv('USERNAME') or 'Unknown'");
        let user: String = py.eval(code, None, Some(&locals))?.extract()?;
        info!(
            "Python status OK! Running user: {}, Python version: {}",
            user, version
        );
        Ok(())
    })
}

pub fn init_client(
    access_token: &str,
    refresh_token: &str,
    user_id: &str,
    cookie: &str,
) -> PyResult<Py<PyAny>> {
    Python::attach(|py| {
        let tgtg_client_fun: Py<PyAny> = PyModule::from_code(
            py,
            c_str!("
from tgtg import TgtgClient
def get_client(access_token, refresh_token, user_id, cookie):
    client = TgtgClient(access_token=access_token, refresh_token=refresh_token, user_id=user_id, cookie=cookie)
    return client"),
            c_str!("client.py"),
            c_str!("client"),
        )?
        .getattr("get_client")?
        .into();
        let args = PyTuple::new(py, [&access_token, &refresh_token, &user_id, &cookie])?;
        let ret: pyo3::Py<pyo3::PyAny> = tgtg_client_fun.call1(py, args)?;
        Ok(ret)
    })
}

/// Area browse with pagination. Includes zero-stock shops and optionally
/// filters by a search phrase. Collects all pages (100 per page) until a short page.
pub fn init_fetch_func() -> PyResult<pyo3::Py<pyo3::PyAny>> {
    Python::attach(|py| {
        let func = PyModule::from_code(
            py,
            c_str!(
                "
import json
def browse_area(client, latitude, longitude, radius, search_phrase):
    results = []
    page = 1
    page_size = 100
    while True:
        items = client.get_items(
            favorites_only=False,
            with_stock_only=False,
            latitude=latitude,
            longitude=longitude,
            radius=radius,
            search_phrase=search_phrase,
            page_size=page_size,
            page=page,
        )
        results.extend(items)
        if len(items) < page_size:
            break
        page += 1
    return json.dumps(results)"
            ),
            c_str!("fetch.py"),
            c_str!("fetch"),
        )?
        .getattr("browse_area")?
        .into();
        Ok(func)
    })
}

/// Single item fetch via `get_item`. Distinguishes a genuinely missing item
/// (API error status 404/410) from transient failures by returning a marker.
pub fn init_item_fetch_func() -> PyResult<pyo3::Py<pyo3::PyAny>> {
    Python::attach(|py| {
        let func = PyModule::from_code(
            py,
            c_str!(
                "
import json
def fetch_item(client, item_id):
    try:
        item = client.get_item(item_id)
        return json.dumps({'found': item})
    except Exception as exc:
        status = getattr(exc, 'status', None)
        if status is None and exc.args:
            status = exc.args[0]
        if status in (400, 404, 410):
            return json.dumps({'not_found': True})
        raise"
            ),
            c_str!("item_fetch.py"),
            c_str!("item_fetch"),
        )?
        .getattr("fetch_item")?
        .into();
        Ok(func)
    })
}

fn py_browse_area_raw(
    tgtg: &TGTGBindings,
    origin: &BrowseOrigin,
    search_phrase: Option<&str>,
) -> PyResult<String> {
    Python::attach(|py| {
        let client: Py<PyAny> = tgtg.client.extract(py)?;
        let phrase: Py<PyAny> = match search_phrase {
            Some(phrase) => PyString::new(py, phrase).into_any().unbind(),
            None => py.None(),
        };
        let ret = tgtg.fetch_func.call1(
            py,
            (
                client,
                origin.latitude,
                origin.longitude,
                origin.radius as i64,
                phrase,
            ),
        )?;
        ret.extract(py)
    })
}

fn py_fetch_item_raw(tgtg: &TGTGBindings, item_id: &str) -> PyResult<String> {
    Python::attach(|py| {
        let client: Py<PyAny> = tgtg.client.extract(py)?;
        let ret = tgtg
            .item_fetch_func
            .call1(py, (client, PyString::new(py, item_id)))?;
        ret.extract(py)
    })
}

/// Browse all listings of an area (including zero stock shops), optionally
/// narrowed down by a search phrase.
pub fn browse_area(
    tgtg_credentials: &TGTGBindings,
    origin: &BrowseOrigin,
    search_phrase: Option<&str>,
) -> anyhow::Result<Vec<TGTGListing>> {
    let py_items = py_browse_area_raw(tgtg_credentials, origin, search_phrase)?;
    let items: Vec<TGTGListing> = serde_json::from_str(&py_items)?;
    Ok(items)
}

/// Fetch a single listing by its item id.
pub fn get_item(tgtg_credentials: &TGTGBindings, item_id: &str) -> anyhow::Result<ItemFetchResult> {
    let py_item = py_fetch_item_raw(tgtg_credentials, item_id)?;
    let value: serde_json::Value = serde_json::from_str(&py_item)?;
    if value
        .get("not_found")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        Ok(ItemFetchResult::NotFound)
    } else {
        let listing: TGTGListing =
            serde_json::from_value(value.get("found").cloned().context("missing found field")?)?;
        Ok(ItemFetchResult::Found(Box::new(listing)))
    }
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
        Python::attach(|py| {
            py.import("tgtg")?;
            Ok(())
        })
    }
}

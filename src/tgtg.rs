use pyo3::ffi::c_str;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyTuple};
use tracing::info;

use crate::data::{TGTGBindings, TGTGConfig, TGTGListing};

pub(crate) fn check_python() -> PyResult<()> {
    Python::with_gil(|py| {
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
) -> PyResult<PyObject> {
    Python::with_gil(|py| {
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
        let ret: PyObject = tgtg_client_fun.call1(py, args)?;
        Ok(ret)
    })
}

pub fn init_fetch_func() -> PyResult<PyObject> {
    Python::with_gil(|py| {
        let func = PyModule::from_code(
            py,
            c_str!("
import json
def fetch_items(client, latitude, longitude, radius):
    items = client.get_items(
        favorites_only=False,
        latitude=latitude,
        longitude=longitude,
        page_size=100,
        radius=radius,
    )
    return json.dumps(items)"),
    c_str!("fetch.py"),
    c_str!("fetch"),
        )?
        .getattr("fetch_items")?
        .into();
        Ok(func)
    })
}

fn py_get_items(tgtg: &TGTGBindings, config: &TGTGConfig) -> PyResult<String> {
    Python::with_gil(|py| {
        let client = tgtg.client.extract(py)?;
        let params = PyTuple::new(
            py,
            [
                format!("{:.5}", config.latitude),
                format!("{:.5}", config.longitude),
                format!("{}", config.radius),
            ],
        )?;
        let args = PyTuple::new(
            py,
            [
                client,
                params.get_item(0)?,
                params.get_item(1)?,
                params.get_item(2)?,
            ],
        )?;
        let ret = tgtg.fetch_func.call1(py, args)?;
        let items = ret.extract::<String>(py)?;
        Ok(items)
    })
}

pub fn get_items(
    tgtg_credentials: &TGTGBindings,
    config: &TGTGConfig,
) -> anyhow::Result<Vec<TGTGListing>> {
    let py_items = py_get_items(tgtg_credentials, config)?;
    let items: Vec<TGTGListing> = serde_json::from_str(&py_items)?;
    Ok(items)
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

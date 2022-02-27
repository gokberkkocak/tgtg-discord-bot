use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyTuple};
use tracing::info;
use tracing::debug;

use crate::{Coordinates, TGTGCredentials};

pub(crate) fn test_python() -> PyResult<()> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let version: String = sys.getattr("version")?.extract()?;
        let locals = [("os", py.import("os")?)].into_py_dict(py);
        let code = "os.getenv('USER') or os.getenv('USERNAME') or 'Unknown'";
        let user: String = py.eval(code, None, Some(locals))?.extract()?;
        info!("Hello {}, I'm Python {}", user, version);
        Ok(())
    })
}

fn get_items(tgtg_credentials: &TGTGCredentials , coords: &Coordinates) -> PyResult<String> {
    Python::with_gil(|py| {
        let fun: Py<PyAny> = PyModule::from_code(
            py,
            "from tgtg import TgtgClient
def fetch_items(access_token, refresh_token, user_id, latitude, longitude):
    client = TgtgClient(access_token=access_token, refresh_token=refresh_token, user_id=user_id)
    items = client.get_items(
        favorites_only=False,
        latitude=latitude,
        longitude=longitude,
        radius=10,
    )
    return str(items)",
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
                &format!("{:.5}", coords.latitude),
                &format!("{:.5}", coords.longitude),
            ],
        );
        let ret = fun.call1(py, args)?;
        let items = ret.extract::<String>(py)?;
        debug!("{}", items);
        Ok(items)
    })
}

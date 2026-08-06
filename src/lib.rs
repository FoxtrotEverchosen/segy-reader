#![warn(clippy::pedantic)]

mod decode;
mod encode;
mod header;
mod segy_file;
mod types;

use crate::encode::{BinaryHeaderConfig, save_segy};
use crate::segy_file::SegyFile;
use pyo3::prelude::*;

#[pymodule]
fn _fastsegy(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SegyFile>()?;
    m.add_class::<BinaryHeaderConfig>()?;
    m.add_function(wrap_pyfunction!(save_segy, m)?)?;
    Ok(())
}

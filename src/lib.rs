#![warn(clippy::pedantic)]

mod types;
mod decode;
mod segy_file;
mod header;
mod encode;

use pyo3::prelude::*;
use crate::encode::{BinaryHeaderConfig, save_segy};
use crate::segy_file::SegyFile;

#[pymodule]
fn _fastsegy(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SegyFile>()?;
    m.add_class::<BinaryHeaderConfig>()?;
    m.add_function(wrap_pyfunction!(save_segy, m)?)?;
    Ok(())
}
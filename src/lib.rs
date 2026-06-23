#![warn(clippy::pedantic)]

mod types;
mod decode;
mod segy_file;
mod header;

use pyo3::prelude::*;

use crate::segy_file::SegyFile;

#[pymodule]
fn _fastsegy(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SegyFile>()?;
    Ok(())
}
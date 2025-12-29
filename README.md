## Fastsegy

Performant SEG-Y reader with Python bindings.  
Enables access to metadata, trace headers, trace data, and trace ranges from SEG-Y files used in seismic and geophysical measurements (e.g., GPR). Combines Rust for fast parsing with Python for easy integration and GUI development.

## Features

- Read SEG-Y textual headers (EBCDIC -> ASCII)  
- Parse binary headers and extract key metadata  
- Iterate through trace headers to receive trace count  
- Python bindings via PyO3 for easy integration 
- Currently supports only SEG-Y Rev 0 and Rev 1 files

## Planned Features

- Efficient handling of large SEG-Y files using Rust  
- Add handling for newer standards (i.e. 2 and possibly 2.1) 
- Create a GUI for better user experience
- Add possibility to read trace data (or ranges of traces) and display them on GUI

## Unlikely Features

- Add basic data processing functionalities (e.g. gain, filters...)

## SEGY data source

Access to free data possible via [seg wikipedia](https://wiki.seg.org/wiki/Open_data)

## Usage, installation and examples

This section will be expanded when a full support for Rev 0/1 data and first GUI iteration are finished. 

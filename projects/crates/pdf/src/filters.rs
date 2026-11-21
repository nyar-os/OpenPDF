//! Stream filter decoding.
//!
//! Only `/FlateDecode` is implemented. Every other filter is reported as
//! unsupported and the stream keeps its raw bytes in the object table, so a
//! caller can never mistake a guess for decoded data.

use std::collections::BTreeMap;

use data_compression::{CodecLimits, Zlib};

use crate::error::{Result, invalid, unsupported};
use crate::object::{Dict, PdfObject, PdfStream};

/// Upper bound for one decoded stream. A malformed document cannot force an
/// unbounded allocation.
pub(crate) const MAX_STREAM_BYTES: u64 = 64 * 1024 * 1024;

/// Decodes a stream through its `/Filter` chain.
///
/// `objects` resolves indirect `/Filter` and `/DecodeParms` values. Unsupported
/// filters return `format/unsupported` without touching the original bytes.
pub(crate) fn decode(stream: &PdfStream, objects: &BTreeMap<u32, PdfObject>) -> Result<Vec<u8>> {
    let filters = filter_names(&stream.dict, objects)?;
    if filters.is_empty() {
        return Ok(stream.data.clone());
    }
    let parameters = decode_parameters(&stream.dict, objects, filters.len())?;

    let mut data = stream.data.clone();
    for (index, filter) in filters.iter().enumerate() {
        let parameters = parameters.get(index).and_then(Option::as_ref);
        match filter.as_str() {
            "FlateDecode" | "Fl" => data = decode_flate(&data, parameters, objects)?,
            "none" => {}
            other => {
                return Err(unsupported(format!("stream filter /{other} is not supported, raw bytes are preserved")));
            }
        }
    }
    Ok(data)
}

fn filter_names(dict: &Dict, objects: &BTreeMap<u32, PdfObject>) -> Result<Vec<String>> {
    let Some(value) = dict.get("Filter") else {
        return Ok(Vec::new());
    };
    let value = resolve(objects, value).ok_or_else(|| invalid("stream /Filter is an unresolvable reference"))?;
    match value {
        PdfObject::Null => Ok(Vec::new()),
        PdfObject::Name(name) if name == "none" => Ok(Vec::new()),
        PdfObject::Name(name) => Ok(vec![name.clone()]),
        PdfObject::Array(items) => {
            if items.is_empty() {
                return Ok(Vec::new());
            }
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                let item =
                    resolve(objects, item).ok_or_else(|| invalid("stream /Filter entry is an unresolvable reference"))?;
                match item {
                    PdfObject::Name(name) => names.push(name.clone()),
                    PdfObject::Null => {}
                    _ => return Err(invalid("stream /Filter array holds a non-name value")),
                }
            }
            Ok(names)
        }
        _ => Err(invalid("stream /Filter is not a name or an array of names")),
    }
}

fn decode_parameters(dict: &Dict, objects: &BTreeMap<u32, PdfObject>, count: usize) -> Result<Vec<Option<Dict>>> {
    let mut parameters = vec![None; count];
    let Some(value) = dict.get("DecodeParms") else {
        return Ok(parameters);
    };
    let value = resolve(objects, value).ok_or_else(|| invalid("stream /DecodeParms is an unresolvable reference"))?;
    match value {
        PdfObject::Null => {}
        PdfObject::Dictionary(entry) => {
            if let Some(slot) = parameters.first_mut() {
                *slot = Some(entry.clone());
            }
        }
        PdfObject::Array(items) => {
            for (index, item) in items.iter().take(count).enumerate() {
                let item =
                    resolve(objects, item).ok_or_else(|| invalid("stream /DecodeParms entry is an unresolvable reference"))?;
                match item {
                    PdfObject::Null => {}
                    PdfObject::Dictionary(entry) => parameters[index] = Some(entry.clone()),
                    _ => return Err(invalid("stream /DecodeParms array holds a non-dictionary value")),
                }
            }
        }
        _ => return Err(invalid("stream /DecodeParms is not a dictionary or an array")),
    }
    Ok(parameters)
}

fn decode_flate(input: &[u8], parameters: Option<&Dict>, objects: &BTreeMap<u32, PdfObject>) -> Result<Vec<u8>> {
    let limits = CodecLimits::new().with_max_output_bytes(MAX_STREAM_BYTES);
    let facts = Zlib::decode(input, limits).map_err(|error| invalid(format!("FlateDecode failed: {error}")))?;
    apply_predictor(facts.value, parameters, objects)
}

fn apply_predictor(data: Vec<u8>, parameters: Option<&Dict>, objects: &BTreeMap<u32, PdfObject>) -> Result<Vec<u8>> {
    let Some(parameters) = parameters else {
        return Ok(data);
    };
    let predictor = integer(parameters, "Predictor", objects).unwrap_or(1);
    if predictor <= 1 {
        return Ok(data);
    }
    let colors = integer(parameters, "Colors", objects).unwrap_or(1);
    let bits = integer(parameters, "BitsPerComponent", objects).unwrap_or(8);
    let columns = integer(parameters, "Columns", objects).unwrap_or(1);
    if colors <= 0 || bits <= 0 || columns <= 0 {
        return Err(invalid("predictor parameters must be positive"));
    }
    if bits != 8 {
        return Err(unsupported(format!("predictor with {bits} bits per component is not supported")));
    }
    let colors = usize::try_from(colors).map_err(|_| invalid("predictor /Colors is too large"))?;
    let columns = usize::try_from(columns).map_err(|_| invalid("predictor /Columns is too large"))?;
    let row_length = colors.checked_mul(columns).ok_or_else(|| invalid("predictor row is too large"))?;
    if row_length == 0 {
        return Ok(data);
    }

    match predictor {
        2 => tiff_predictor(data, colors, row_length),
        10..=15 => png_predictor(&data, colors, row_length),
        other => Err(unsupported(format!("predictor {other} is not supported"))),
    }
}

/// TIFF predictor 2: each byte is the difference from the pixel `colors` bytes
/// to its left in the same row.
fn tiff_predictor(mut data: Vec<u8>, colors: usize, row_length: usize) -> Result<Vec<u8>> {
    if data.len() % row_length != 0 {
        return Err(invalid("TIFF predictor data is not a whole number of rows"));
    }
    for row in data.chunks_mut(row_length) {
        for index in colors..row.len() {
            row[index] = row[index].wrapping_add(row[index - colors]);
        }
    }
    Ok(data)
}

fn png_predictor(data: &[u8], colors: usize, row_length: usize) -> Result<Vec<u8>> {
    let stride = row_length.checked_add(1).ok_or_else(|| invalid("predictor row is too large"))?;
    if data.len() % stride != 0 {
        return Err(invalid("PNG predictor data is not a whole number of rows"));
    }

    let total = data.len() / stride * row_length;
    let mut output = Vec::with_capacity(total);
    let mut previous = vec![0u8; row_length];
    for row in data.chunks_exact(stride) {
        let filter = row[0];
        let raw = &row[1..];
        let mut current = raw.to_vec();
        match filter {
            0 => {}
            1 => {
                for index in colors..row_length {
                    current[index] = current[index].wrapping_add(current[index - colors]);
                }
            }
            2 => {
                for index in 0..row_length {
                    current[index] = current[index].wrapping_add(previous[index]);
                }
            }
            3 => {
                for index in 0..row_length {
                    let left = if index >= colors { u16::from(current[index - colors]) } else { 0 };
                    let up = u16::from(previous[index]);
                    current[index] = current[index].wrapping_add(((left + up) / 2) as u8);
                }
            }
            4 => {
                for index in 0..row_length {
                    let left = if index >= colors { current[index - colors] } else { 0 };
                    let up = previous[index];
                    let up_left = if index >= colors { previous[index - colors] } else { 0 };
                    current[index] = current[index].wrapping_add(paeth(left, up, up_left));
                }
            }
            other => return Err(invalid(format!("PNG predictor row filter {other} is invalid"))),
        }
        output.extend_from_slice(&current);
        previous = current;
    }
    Ok(output)
}

fn paeth(left: u8, up: u8, up_left: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let up_left = i32::from(up_left);
    let estimate = left + up - up_left;
    let left_distance = (estimate - left).abs();
    let up_distance = (estimate - up).abs();
    let corner_distance = (estimate - up_left).abs();
    if left_distance <= up_distance && left_distance <= corner_distance {
        left as u8
    } else if up_distance <= corner_distance {
        up as u8
    } else {
        up_left as u8
    }
}

fn integer(dict: &Dict, key: &str, objects: &BTreeMap<u32, PdfObject>) -> Option<i64> {
    let value = dict.get(key)?;
    match resolve(objects, value)? {
        PdfObject::Integer(value) => Some(*value),
        _ => None,
    }
}

fn resolve<'a>(objects: &'a BTreeMap<u32, PdfObject>, value: &'a PdfObject) -> Option<&'a PdfObject> {
    let mut current = value;
    for _ in 0..16 {
        match current {
            PdfObject::Reference(number, _) => current = objects.get(number)?,
            _ => return Some(current),
        }
    }
    None
}

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use data_compression::{DeflateLevel, Zlib};
use open_pdf_core::{CreateRequest, capabilities, create, extract_images, extract_text, inspect, merge, split};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let sequence = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("open-pdf-tests-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&root).expect("create test workspace");
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, bytes).expect("write test PDF");
        path
    }

    fn create(&self, name: &str, title: Option<&str>, pages: &[&str]) -> PathBuf {
        let path = self.path(name);
        let report = create(&CreateRequest {
            out: path.clone(),
            title: title.map(str::to_string),
            pages: pages.iter().map(|page| (*page).to_string()).collect(),
            overwrite: true,
        })
        .expect("create PDF");
        assert_eq!(report.pages, pages.len());
        assert!(report.bytes > 0);
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_invalid<T: std::fmt::Debug>(result: Result<T, open_pdf_core::ServiceError>, expected: &str) {
    let error = result.expect_err("expected a failure");
    assert_eq!(error.code(), expected, "unexpected error: {error}");
}

fn assemble(objects: &[&str]) -> Vec<u8> {
    let mut output = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
    }
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    output
}

#[test]
fn create_then_inspect_reports_pages_title_and_a4_geometry() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", Some("Quarterly deck"), &["Alpha", "Beta"]);

    let report = inspect(&path).expect("inspect created PDF");
    assert_eq!(report.label, "deck.pdf");
    assert_eq!(report.version, "1.7");
    assert_eq!(report.pages, 2);
    assert!(!report.encrypted);
    assert_eq!(report.metadata.title.as_deref(), Some("Quarterly deck"));
    assert_eq!(report.page_sizes.len(), 2);
    for page in &report.page_sizes {
        assert_eq!(page.width_pt, 612.0);
        assert_eq!(page.height_pt, 792.0);
        assert_eq!(page.rotation, 0);
    }
    assert!(report.bookmarks.is_empty());
}

#[test]
fn split_writes_one_file_per_page() {
    let workspace = Workspace::new();
    let path = workspace.create("two.pdf", None, &["Alpha", "Beta"]);
    let out_dir = workspace.path("parts");

    let report = split(&path, "1,2", &out_dir).expect("split");
    assert_eq!(report.documents.len(), 2);
    assert_eq!(report.documents[0].pages, 1);
    assert_eq!(report.documents[1].pages, 1);
    assert_eq!(Path::new(&report.documents[0].path).file_name().and_then(|name| name.to_str()), Some("two-p1.pdf"));
    assert_eq!(Path::new(&report.documents[1].path).file_name().and_then(|name| name.to_str()), Some("two-p2.pdf"));

    for (index, part) in report.documents.iter().enumerate() {
        let inspected = inspect(Path::new(&part.path)).expect("inspect part");
        assert_eq!(inspected.pages, 1);
        let extracted = extract_text(Path::new(&part.path)).expect("extract part");
        let expected = if index == 0 { "Alpha" } else { "Beta" };
        assert!(extracted.markdown.contains(expected), "missing {expected} in {:?}", extracted.markdown);
    }
}

#[test]
fn split_keeps_ranges_in_order_and_names_multi_page_files() {
    let workspace = Workspace::new();
    let path = workspace.create("book.pdf", None, &["One", "Two", "Three", "Four"]);
    let out_dir = workspace.path("ranges");

    let report = split(&path, "3-4,1", &out_dir).expect("split ranges");
    assert_eq!(report.documents.len(), 2);
    assert_eq!(report.documents[0].pages, 2);
    assert_eq!(report.documents[1].pages, 1);
    assert!(report.documents[0].path.ends_with("book-p3-4.pdf"));
    assert!(report.documents[1].path.ends_with("book-p1.pdf"));

    let extracted = extract_text(Path::new(&report.documents[0].path)).expect("extract range");
    let three = extracted.markdown.find("Three").expect("Three");
    let four = extracted.markdown.find("Four").expect("Four");
    assert!(three < four);
}

#[test]
fn merge_concatenates_pages_in_input_order_and_uses_the_first_info() {
    let workspace = Workspace::new();
    let first = workspace.create("first.pdf", Some("First title"), &["AAA", "BBB"]);
    let second = workspace.create("second.pdf", Some("Second title"), &["CCC"]);
    let merged = workspace.path("merged.pdf");

    let report = merge(&[first, second], &merged, false, None).expect("merge");
    assert_eq!(report.pages, 3);

    let inspected = inspect(&merged).expect("inspect merged");
    assert_eq!(inspected.pages, 3);
    assert_eq!(inspected.metadata.title.as_deref(), Some("First title"));

    let extracted = extract_text(&merged).expect("extract merged");
    let aaa = extracted.markdown.find("AAA").expect("AAA");
    let bbb = extracted.markdown.find("BBB").expect("BBB");
    let ccc = extracted.markdown.find("CCC").expect("CCC");
    assert!(aaa < bbb && bbb < ccc, "pages are out of order: {:?}", extracted.markdown);
}

#[test]
fn merge_title_override_wins() {
    let workspace = Workspace::new();
    let first = workspace.create("one.pdf", Some("Original"), &["Only"]);
    let merged = workspace.path("titled.pdf");

    merge(&[first], &merged, false, Some("Overridden")).expect("merge");
    let inspected = inspect(&merged).expect("inspect");
    assert_eq!(inspected.metadata.title.as_deref(), Some("Overridden"));
}

#[test]
fn merge_refuses_to_overwrite_without_the_flag() {
    let workspace = Workspace::new();
    let first = workspace.create("input.pdf", None, &["Page"]);
    let merged = workspace.path("exists.pdf");
    fs::write(&merged, b"placeholder").expect("seed output");

    assert_invalid(merge(&[first], &merged, false, None), "io/exists");
}

#[test]
fn malformed_bytes_report_format_invalid_without_panicking() {
    let workspace = Workspace::new();
    let broken = workspace.path("broken.pdf");
    fs::write(&broken, b"this is not a PDF at all").expect("write broken file");

    assert_invalid(inspect(&broken), "format/invalid");
    assert_invalid(extract_text(&broken), "format/invalid");
}

#[test]
fn encrypted_trailer_is_rejected_as_unsupported() {
    let workspace = Workspace::new();
    let path = workspace.create("plain.pdf", None, &["Secret"]);
    let bytes = fs::read(&path).expect("read created PDF");
    let marker = b"trailer\n<<";
    let index = bytes.windows(marker.len()).position(|window| window == marker).expect("trailer marker");
    let insert_at = index + marker.len();
    let mut encrypted = Vec::with_capacity(bytes.len() + 20);
    encrypted.extend_from_slice(&bytes[..insert_at]);
    encrypted.extend_from_slice(b" /Encrypt 1 0 R");
    encrypted.extend_from_slice(&bytes[insert_at..]);

    let encrypted_path = workspace.path("encrypted.pdf");
    fs::write(&encrypted_path, encrypted).expect("write encrypted sample");
    assert_invalid(inspect(&encrypted_path), "format/unsupported");
}

#[test]
fn range_syntax_rejects_zero_reversed_and_out_of_bounds() {
    let workspace = Workspace::new();
    let path = workspace.create("two.pdf", None, &["One", "Two"]);
    let out_dir = workspace.path("bad-ranges");

    for ranges in ["", "0", "3", "2-1", "1,", "first"] {
        assert_invalid(split(&path, ranges, &out_dir), "session/argument-invalid");
    }
}

#[test]
fn inspect_reads_nested_outlines_and_inherited_page_attributes() {
    let workspace = Workspace::new();
    let path = workspace.path("outlined.pdf");
    let bytes = assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] /Rotate 90 >>",
        "<< /Type /Page /Parent 2 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        "<< /Type /Outlines /First 6 0 R /Last 8 0 R /Count 3 >>",
        "<< /Title (Intro) /Parent 5 0 R /First 7 0 R /Next 8 0 R >>",
        "<< /Title (Details) /Parent 6 0 R >>",
        "<< /Title (End) /Parent 5 0 R /Next 6 0 R >>",
    ]);
    fs::write(&path, bytes).expect("write outlined PDF");

    let report = inspect(&path).expect("inspect outlined PDF");
    assert_eq!(report.pages, 1);
    assert_eq!(report.page_sizes[0].width_pt, 300.0);
    assert_eq!(report.page_sizes[0].height_pt, 400.0);
    assert_eq!(report.page_sizes[0].rotation, 90);
    let bookmarks: Vec<(&str, usize)> =
        report.bookmarks.iter().map(|bookmark| (bookmark.title.as_str(), bookmark.depth)).collect();
    assert_eq!(bookmarks, [("Intro", 1), ("Details", 2), ("End", 1)]);
}

#[test]
fn xref_stream_with_an_empty_page_tree_is_accepted() {
    let workspace = Workspace::new();
    let path = workspace.path("xref-stream.pdf");

    let mut output = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for body in ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [] /Count 0 >>"] {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", offsets.len()).as_bytes());
    }
    let xref_offset = output.len();
    let mut entries = Vec::new();
    entries.extend_from_slice(&[0, 0, 0, 0]);
    for offset in &offsets {
        entries.push(1);
        entries.extend_from_slice(&(*offset as u16).to_be_bytes());
        entries.push(0);
    }
    entries.push(1);
    entries.extend_from_slice(&(xref_offset as u16).to_be_bytes());
    entries.push(0);
    output.extend_from_slice(
        format!("3 0 obj\n<< /Type /XRef /Size 4 /W [1 2 1] /Index [0 4] /Root 1 0 R /Length {} >>\nstream\n", entries.len())
            .as_bytes(),
    );
    output.extend_from_slice(&entries);
    output.extend_from_slice(b"\nendstream\nendobj\n");
    output.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    fs::write(&path, output).expect("write xref stream PDF");

    let report = inspect(&path).expect("inspect xref stream PDF");
    assert_eq!(report.pages, 0);
}

fn assemble_bytes(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut output = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(body);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    output
}

fn filtered_pdf(filter: &str, decode_parms: &str, data: &[u8]) -> Vec<u8> {
    let mut stream = format!("<< /Length {} /Filter {filter}{decode_parms} >>\nstream\n", data.len()).into_bytes();
    stream.extend_from_slice(data);
    stream.extend_from_slice(b"\nendstream");
    assemble_bytes(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R >>".to_vec(),
        stream,
    ])
}

fn ascii_hex_encode(data: &[u8]) -> Vec<u8> {
    let mut output = String::new();
    for byte in data {
        output.push_str(&format!("{byte:02X}"));
    }
    output.push('>');
    output.into_bytes()
}

fn ascii85_encode(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for group in data.chunks(4) {
        let mut padded = [0u8; 4];
        padded[..group.len()].copy_from_slice(group);
        let value = u32::from_be_bytes(padded);
        if group.len() == 4 && value == 0 {
            output.push(b'z');
            continue;
        }
        let mut digits = [0u8; 5];
        let mut rest = value;
        for index in (0..5).rev() {
            digits[index] = (rest % 85) as u8;
            rest /= 85;
        }
        let take = if group.len() == 4 { 5 } else { group.len() + 1 };
        for digit in &digits[..take] {
            output.push(digit + b'!');
        }
    }
    output.extend_from_slice(b"~>");
    output
}

fn run_length_encode(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for run in data.chunks(128) {
        output.push((run.len() - 1) as u8);
        output.extend_from_slice(run);
    }
    output.push(128);
    output
}

struct BitWriter {
    output: Vec<u8>,
    current: u8,
    used: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self { output: Vec::new(), current: 0, used: 0 }
    }

    fn write(&mut self, value: u16, width: usize) {
        for shift in (0..width).rev() {
            let bit = ((value >> shift) & 1) as u8;
            self.current = (self.current << 1) | bit;
            self.used += 1;
            if self.used == 8 {
                self.output.push(self.current);
                self.current = 0;
                self.used = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.used > 0 {
            self.current <<= 8 - self.used;
            self.output.push(self.current);
        }
        self.output
    }
}

/// Encodes with the same width schedule a decoder follows, so `/EarlyChange`
/// can be exercised without reusing the decoder under test.
fn lzw_encode(data: &[u8], early_change: usize) -> Vec<u8> {
    fn initial() -> HashMap<Vec<u8>, u16> {
        (0..=255u16).map(|byte| (vec![byte as u8], byte)).collect()
    }
    fn emit(writer: &mut BitWriter, code: u16, width: &mut usize, decoder_next: &mut usize, emitted: &mut usize, early: usize) {
        writer.write(code, *width);
        if *emitted > 0 {
            *decoder_next += 1;
            if *width < 12 && *decoder_next + early >= (1 << *width) {
                *width += 1;
            }
        }
        *emitted += 1;
    }

    let mut table = initial();
    let mut next: u16 = 258;
    let mut width = 9usize;
    let mut decoder_next = 258usize;
    let mut emitted = 0usize;
    let mut writer = BitWriter::new();
    writer.write(256, width);

    let mut current: Vec<u8> = Vec::new();
    for &byte in data {
        let mut candidate = current.clone();
        candidate.push(byte);
        if table.contains_key(&candidate) {
            current = candidate;
            continue;
        }
        let code = table[&current];
        emit(&mut writer, code, &mut width, &mut decoder_next, &mut emitted, early_change);
        if next < 4096 {
            table.insert(candidate, next);
            next += 1;
        } else {
            writer.write(256, width);
            table = initial();
            next = 258;
            width = 9;
            decoder_next = 258;
            emitted = 0;
        }
        current = vec![byte];
    }
    if !current.is_empty() {
        emit(&mut writer, table[&current], &mut width, &mut decoder_next, &mut emitted, early_change);
    }
    writer.write(257, width);
    writer.finish()
}

#[test]
fn ascii_hex_ascii85_run_length_and_lzw_content_streams_are_extracted() {
    let workspace = Workspace::new();
    let plain = b"BT /F1 12 Tf 72 720 Td (Filtered text) Tj ET\n".to_vec();
    let cases = [
        ("/ASCIIHexDecode", String::new(), ascii_hex_encode(&plain)),
        ("/ASCII85Decode", String::new(), ascii85_encode(&plain)),
        ("/RunLengthDecode", String::new(), run_length_encode(&plain)),
        ("/LZWDecode", String::new(), lzw_encode(&plain, 1)),
        ("/LZWDecode", " /DecodeParms << /EarlyChange 0 >>".to_string(), lzw_encode(&plain, 0)),
    ];

    for (index, (filter, decode_parms, data)) in cases.iter().enumerate() {
        let path = workspace.write(&format!("filter-{index}.pdf"), &filtered_pdf(filter, decode_parms, data));
        let extracted = extract_text(&path).unwrap_or_else(|error| panic!("{filter}: {error}"));
        assert!(extracted.markdown.contains("Filtered text"), "{filter}: {:?}", extracted.markdown);
        assert!(extracted.losses.is_empty(), "{filter}: {:?}", extracted.losses);
    }
}

#[test]
fn filter_arrays_apply_in_order() {
    let workspace = Workspace::new();
    let plain = b"BT /F1 12 Tf 72 720 Td (Chained filters work) Tj ET\n".to_vec();
    let compressed = Zlib::encode(&plain, DeflateLevel::Default);
    let data = ascii85_encode(&compressed);
    let path = workspace.write("chain.pdf", &filtered_pdf("[/ASCII85Decode /FlateDecode]", "", &data));

    let extracted = extract_text(&path).expect("extract chained filters");
    assert!(extracted.markdown.contains("Chained filters work"), "{:?}", extracted.markdown);
    assert!(extracted.losses.is_empty(), "{:?}", extracted.losses);
}

#[test]
fn lzw_width_growth_decodes_with_both_early_change_values() {
    let workspace = Workspace::new();
    let words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel"];
    let mut text = String::new();
    for index in 0..400 {
        text.push_str(words[index % words.len()]);
        text.push(' ');
    }
    let plain = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET\n").into_bytes();

    for (index, early) in [1usize, 0usize].into_iter().enumerate() {
        let data = lzw_encode(&plain, early);
        let decode_parms = format!(" /DecodeParms << /EarlyChange {early} >>");
        let path = workspace.write(&format!("lzw-wide-{index}.pdf"), &filtered_pdf("/LZWDecode", &decode_parms, &data));
        let extracted = extract_text(&path).unwrap_or_else(|error| panic!("early {early}: {error}"));
        assert!(
            extracted.markdown.starts_with("alpha bravo"),
            "early {early}: {:?}",
            &extracted.markdown[..32.min(extracted.markdown.len())]
        );
        assert!(extracted.losses.is_empty(), "early {early}: {:?}", extracted.losses);
    }
}

fn image_pdf() -> Vec<u8> {
    let rgb = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    let alpha = vec![0u8, 85, 170, 255];
    let gray = vec![200u8];
    let rgb_data = Zlib::encode(&rgb, DeflateLevel::Default);
    let alpha_data = Zlib::encode(&alpha, DeflateLevel::Default);
    let gray_data = Zlib::encode(&gray, DeflateLevel::Default);

    let mut rgb_stream = format!(
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB \
         /SMask 6 0 R /Filter /FlateDecode /Length {} >>\nstream\n",
        rgb_data.len()
    )
    .into_bytes();
    rgb_stream.extend_from_slice(&rgb_data);
    rgb_stream.extend_from_slice(b"\nendstream");

    let mut alpha_stream = format!(
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceGray \
         /Filter /FlateDecode /Length {} >>\nstream\n",
        alpha_data.len()
    )
    .into_bytes();
    alpha_stream.extend_from_slice(&alpha_data);
    alpha_stream.extend_from_slice(b"\nendstream");

    let mut gray_stream = format!(
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray \
         /Filter /FlateDecode /Length {} >>\nstream\n",
        gray_data.len()
    )
    .into_bytes();
    gray_stream.extend_from_slice(&gray_data);
    gray_stream.extend_from_slice(b"\nendstream");

    let jpeg = vec![0xFFu8, 0xD8, 0xFF, 0xD9];

    assemble_bytes(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << /XObject << \
          /Im1 5 0 R /Jpeg 7 0 R /BadJpeg 8 0 R /Fm1 4 0 R >> >> >>"
            .to_vec(),
        b"<< /Type /XObject /Subtype /Form /BBox [0 0 2 2] /Resources << /XObject << /Inner 9 0 R >> >> \
          /Length 0 >>\nstream\n\nendstream"
            .to_vec(),
        rgb_stream,
        alpha_stream,
        {
            let mut body = format!(
                "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB \
                 /Filter /DCTDecode /Length {} >>\nstream\n",
                jpeg.len()
            )
            .into_bytes();
            body.extend_from_slice(&jpeg);
            body.extend_from_slice(b"\nendstream");
            body
        },
        b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB \
          /Filter /DCTDecode /Length 8 >>\nstream\nnotajpeg\nendstream"
            .to_vec(),
        gray_stream,
    ])
}

fn png_dimensions(bytes: &[u8]) -> (u32, u32, u8, u8) {
    assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], "PNG signature");
    assert_eq!(&bytes[12..16], b"IHDR");
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("height"));
    assert!(bytes.windows(4).any(|window| window == b"IDAT"), "IDAT chunk");
    assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND", "IEND chunk");
    (width, height, bytes[24], bytes[25])
}

#[test]
fn extract_images_writes_pngs_with_smask_and_keeps_jpegs() {
    let workspace = Workspace::new();
    let path = workspace.write("images.pdf", &image_pdf());
    let out_dir = workspace.path("images");

    let images = extract_images(&path, &out_dir).expect("extract images");
    assert_eq!(images.len(), 4);

    let rgb = images.iter().find(|image| image.name == "Im1").expect("RGB image");
    assert_eq!(rgb.filter, "FlateDecode");
    assert_eq!((rgb.width, rgb.height), (2, 2));
    assert!(rgb.loss.is_none(), "{:?}", rgb.loss);
    assert!(rgb.path.ends_with(".png"), "{}", rgb.path);
    let png = fs::read(&rgb.path).expect("read PNG");
    let (width, height, depth, color_type) = png_dimensions(&png);
    assert_eq!((width, height), (2, 2));
    assert_eq!(depth, 8);
    assert_eq!(color_type, 6, "RGBA from /SMask");

    let inner = images.iter().find(|image| image.name == "Inner").expect("nested form image");
    assert!(inner.loss.is_none(), "{:?}", inner.loss);
    assert!(inner.path.ends_with(".png"));
    let (_, _, _, color_type) = png_dimensions(&fs::read(&inner.path).expect("read nested PNG"));
    assert_eq!(color_type, 0, "DeviceGray PNG");

    let jpeg = images.iter().find(|image| image.name == "Jpeg").expect("JPEG image");
    assert_eq!(jpeg.filter, "DCTDecode");
    assert!(jpeg.path.ends_with(".jpg"));
    assert_eq!(fs::read(&jpeg.path).expect("read JPEG"), vec![0xFF, 0xD8, 0xFF, 0xD9]);

    let bad = images.iter().find(|image| image.name == "BadJpeg").expect("invalid DCT entry");
    assert!(bad.path.is_empty());
    let loss = bad.loss.as_deref().expect("invalid DCT must report a loss");
    assert!(loss.contains("SOI"), "{loss}");
}

#[test]
fn acroform_fields_report_qualified_names_flags_and_values() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "form.pdf",
        &assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm 7 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /T (person) /FT /Tx /Kids [5 0 R] >>",
            "<< /T (name) /Parent 4 0 R /V (Alice) >>",
            "<< /T (agree) /FT /Btn /Ff 3 /V /Yes >>",
            "<< /Fields [4 0 R 6 0 R] /DA (/Helv 0 Tf 0 g) >>",
        ]),
    );

    let report = inspect(&path).expect("inspect form PDF");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.form_fields.len(), 2);

    let name = &report.form_fields[0];
    assert_eq!(name.name, "person.name");
    assert_eq!(name.field_type.as_deref(), Some("Tx"));
    assert_eq!(name.value.as_deref(), Some("Alice"));
    assert!(!name.read_only);
    assert!(!name.required);

    let agree = &report.form_fields[1];
    assert_eq!(agree.name, "agree");
    assert_eq!(agree.field_type.as_deref(), Some("Btn"));
    assert_eq!(agree.value.as_deref(), Some("Yes"));
    assert!(agree.read_only);
    assert!(agree.required);
}

#[test]
fn corrupt_resources_and_form_cycles_report_warnings_without_panicking() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "broken-form.pdf",
        &assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /XObject << \
              /Loop 4 0 R /Missing 99 0 R >> >> >>",
            "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Loop 4 0 R >> >> \
              /Length 0 >>\nstream\n\nendstream",
            "<< /Fields [6 0 R] >>",
            "<< /T (loop) /Kids [7 0 R] >>",
            "<< /T (inner) /Kids [6 0 R] >>",
        ]),
    );

    let report = inspect(&path).expect("inspect broken form");
    assert_eq!(report.pages, 1);
    assert!(report.warnings.iter().any(|warning| warning.contains("cycle")), "{:?}", report.warnings);

    let images = extract_images(&path, &workspace.path("broken-images")).expect("extract broken images");
    assert!(images.is_empty(), "{images:?}");
}

#[test]
fn capabilities_disclose_new_filters_forms_and_image_extraction() {
    let matrix = capabilities();
    for filter in ["asciiHexDecode", "ascii85Decode", "runLengthDecode", "lzwDecode"] {
        assert_eq!(matrix["formats"]["pdf"]["filters"][filter], "supported", "{filter}");
    }
    assert_eq!(matrix["formats"]["pdf"]["structures"]["acroForm"], "supported");
    assert_eq!(matrix["operations"]["extractImages"], "supported");
}

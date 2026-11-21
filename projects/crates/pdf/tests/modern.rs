use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use data_compression::{DeflateLevel, Zlib};
use open_pdf_core::{extract_text, inspect, merge, split_every};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let sequence = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("open-pdf-modern-{}-{sequence}", std::process::id()));
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
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Predictor {
    None,
    Png,
    Tiff,
}

struct ModernPdf {
    title: String,
    pages: Vec<String>,
    encrypted: bool,
    predictor: Predictor,
}

impl ModernPdf {
    fn new(title: &str, pages: &[&str]) -> Self {
        Self {
            title: title.to_string(),
            pages: pages.iter().map(|page| (*page).to_string()).collect(),
            encrypted: false,
            predictor: Predictor::None,
        }
    }
}

#[derive(Clone, Copy)]
enum Row {
    Free,
    InFile(u64),
    InStream(u32, u32),
}

fn build_modern_pdf(spec: &ModernPdf) -> Vec<u8> {
    let page_count = spec.pages.len();
    assert!(page_count > 0);

    let catalog = 1u32;
    let pages_root = 2u32;
    let font = 3u32;
    let first_page = 4u32;
    let first_content = first_page + page_count as u32;
    let object_stream = first_content + page_count as u32;
    let xref = object_stream + 1;
    let info = xref + 1;
    let length_object = info + 1;
    let size = length_object + 1;

    let mut embedded: Vec<(u32, Vec<u8>)> = Vec::new();
    embedded.push((catalog, format!("<< /Type /Catalog /Pages {pages_root} 0 R >>").into_bytes()));
    let kids = (0..page_count).map(|index| format!("{} 0 R", first_page + index as u32)).collect::<Vec<_>>().join(" ");
    embedded.push((pages_root, format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>").into_bytes()));
    embedded.push((font, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec()));

    let mut contents = Vec::with_capacity(page_count);
    for text in &spec.pages {
        contents.push(Zlib::encode(page_content(text).as_bytes(), DeflateLevel::Default));
    }

    let mut documents = Vec::new();
    for index in 0..page_count {
        let number = first_page + index as u32;
        let content = first_content + index as u32;
        documents.push((
            number,
            format!(
                "<< /Type /Page /Parent {pages_root} 0 R /MediaBox [0 0 300 400] /Contents {content} 0 R \
                 /Resources << /Font << /F1 {font} 0 R >> >> >>"
            )
            .into_bytes(),
        ));
    }

    let info_body = format!("<< /Title ({}) /Author (OpenPDF modern fixture) >>", escape_text(&spec.title));
    embedded.push((info, info_body.into_bytes()));
    embedded.push((length_object, contents[0].len().to_string().into_bytes()));
    embedded.extend(documents);
    embedded.sort_by_key(|(number, _)| *number);

    let mut header = String::new();
    let mut bodies = Vec::new();
    for (index, (number, body)) in embedded.iter().enumerate() {
        header.push_str(&format!("{number} {} ", bodies.len()));
        bodies.extend_from_slice(body);
        bodies.push(b'\n');
        assert_eq!(*number, embedded_number(index, catalog, pages_root, font, first_page, page_count, info, length_object));
    }
    let first = header.len();
    let mut decoded = header.into_bytes();
    decoded.extend_from_slice(&bodies);
    let object_stream_data = Zlib::encode(&decoded, DeflateLevel::Default);

    let mut output = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut rows = vec![Row::Free; size as usize];

    for (index, data) in contents.iter().enumerate() {
        let number = first_content + index as u32;
        rows[number as usize] = Row::InFile(output.len() as u64);
        // The first stream proves that an indirect `/Length` inside an object
        // stream is resolved before the body is sliced.
        let length = if index == 0 { format!("{length_object} 0 R") } else { data.len().to_string() };
        output.extend_from_slice(format!("{number} 0 obj\n<< /Length {length} /Filter /FlateDecode >>\nstream\n").as_bytes());
        output.extend_from_slice(data);
        output.extend_from_slice(b"\nendstream\nendobj\n");
    }

    rows[object_stream as usize] = Row::InFile(output.len() as u64);
    output.extend_from_slice(
        format!(
            "{object_stream} 0 obj\n<< /Type /ObjStm /N {} /First {first} /Filter /FlateDecode /Length {} >>\nstream\n",
            embedded.len(),
            object_stream_data.len()
        )
        .as_bytes(),
    );
    output.extend_from_slice(&object_stream_data);
    output.extend_from_slice(b"\nendstream\nendobj\n");

    for (index, (number, _)) in embedded.iter().enumerate() {
        rows[*number as usize] = Row::InStream(object_stream, index as u32);
    }

    let xref_offset = output.len() as u64;
    rows[xref as usize] = Row::InFile(xref_offset);

    let mut entries = Vec::with_capacity(rows.len() * 7);
    for row in &rows {
        let (kind, field2, field3) = match row {
            Row::Free => (0u8, 0u64, 0u64),
            Row::InFile(offset) => (1, *offset, 0),
            Row::InStream(stream, index) => (2, u64::from(*stream), u64::from(*index)),
        };
        entries.push(kind);
        entries.extend_from_slice(&(field2 as u32).to_be_bytes());
        entries.extend_from_slice(&(field3 as u16).to_be_bytes());
    }
    let encoded = encode_predictor(&entries, 7, spec.predictor);
    let compressed = Zlib::encode(&encoded, DeflateLevel::Default);

    let encrypt = if spec.encrypted { " /Encrypt 99 0 R" } else { "" };
    let decode_parms = match spec.predictor {
        Predictor::None => String::new(),
        Predictor::Png => " /DecodeParms << /Predictor 12 /Columns 7 >>".to_string(),
        Predictor::Tiff => " /DecodeParms << /Predictor 2 /Columns 7 >>".to_string(),
    };
    output.extend_from_slice(
        format!(
            "{xref} 0 obj\n<< /Type /XRef /Size {size} /W [1 4 2] /Index [0 {size}] /Root {catalog} 0 R \
             /Info {info} 0 R{encrypt} /Filter /FlateDecode{decode_parms} /Length {} >>\nstream\n",
            compressed.len()
        )
        .as_bytes(),
    );
    output.extend_from_slice(&compressed);
    output.extend_from_slice(b"\nendstream\nendobj\n");
    output.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    output
}

fn embedded_number(
    index: usize,
    catalog: u32,
    pages_root: u32,
    font: u32,
    first_page: u32,
    page_count: usize,
    info: u32,
    length_object: u32,
) -> u32 {
    match index {
        0 => catalog,
        1 => pages_root,
        2 => font,
        _ if index < 3 + page_count => first_page + (index - 3) as u32,
        _ if index == 3 + page_count => info,
        _ => length_object,
    }
}

fn encode_predictor(entries: &[u8], columns: usize, predictor: Predictor) -> Vec<u8> {
    match predictor {
        Predictor::None => entries.to_vec(),
        Predictor::Png => {
            let mut output = Vec::with_capacity(entries.len() + entries.len() / columns);
            let mut previous = vec![0u8; columns];
            for row in entries.chunks_exact(columns) {
                output.push(2);
                for (index, byte) in row.iter().enumerate() {
                    output.push(byte.wrapping_sub(previous[index]));
                }
                previous.copy_from_slice(row);
            }
            output
        }
        Predictor::Tiff => {
            let mut output = Vec::with_capacity(entries.len());
            for row in entries.chunks_exact(columns) {
                output.push(row[0]);
                for index in 1..columns {
                    output.push(row[index].wrapping_sub(row[index - 1]));
                }
            }
            output
        }
    }
}

fn page_content(text: &str) -> String {
    format!("BT\n/F1 12 Tf\n72 720 Td\n({}) Tj\nET\n", escape_text(text))
}

fn escape_text(text: &str) -> String {
    let mut escaped = String::new();
    for character in text.chars() {
        match character {
            '(' => escaped.push_str("\\("),
            ')' => escaped.push_str("\\)"),
            '\\' => escaped.push_str("\\\\"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[test]
fn xref_stream_object_stream_and_flate_content_are_read() {
    let workspace = Workspace::new();
    let bytes = build_modern_pdf(&ModernPdf::new("Modern report", &["Hello modern PDF"]));
    let path = workspace.write("modern.pdf", &bytes);

    let report = inspect(&path).expect("inspect modern PDF");
    assert_eq!(report.pages, 1);
    assert_eq!(report.version, "1.7");
    assert_eq!(report.metadata.title.as_deref(), Some("Modern report"));
    assert_eq!(report.metadata.author.as_deref(), Some("OpenPDF modern fixture"));
    assert_eq!(report.page_sizes[0].width_pt, 300.0);
    assert_eq!(report.page_sizes[0].height_pt, 400.0);

    let extracted = extract_text(&path).expect("extract modern PDF");
    assert!(extracted.markdown.contains("Hello modern PDF"), "missing text: {:?}", extracted.markdown);
    assert!(extracted.losses.is_empty(), "unexpected losses: {:?}", extracted.losses);
}

#[test]
fn merge_keeps_modern_pdf_page_order() {
    let workspace = Workspace::new();
    let first = workspace.write("modern-a.pdf", &build_modern_pdf(&ModernPdf::new("A", &["AAA modern"])));
    let second = workspace.write("modern-b.pdf", &build_modern_pdf(&ModernPdf::new("B", &["BBB modern"])));
    let merged = workspace.path("merged-modern.pdf");

    let report = merge(&[first, second], &merged, false, None).expect("merge modern PDFs");
    assert_eq!(report.pages, 2);

    let inspected = inspect(&merged).expect("inspect merged modern PDF");
    assert_eq!(inspected.pages, 2);
    assert_eq!(inspected.metadata.title.as_deref(), Some("A"));

    let extracted = extract_text(&merged).expect("extract merged modern PDF");
    let aaa = extracted.markdown.find("AAA modern").expect("AAA");
    let bbb = extracted.markdown.find("BBB modern").expect("BBB");
    assert!(aaa < bbb, "pages are out of order: {:?}", extracted.markdown);
}

#[test]
fn split_every_groups_consecutive_pages_of_a_modern_pdf() {
    let workspace = Workspace::new();
    let bytes = build_modern_pdf(&ModernPdf::new("Five", &["One", "Two", "Three", "Four", "Five"]));
    let path = workspace.write("five.pdf", &bytes);

    let out_dir = workspace.path("chunks");
    let report = split_every(&path, 2, &out_dir).expect("split every two pages");
    assert_eq!(report.documents.len(), 3);
    let pages: Vec<usize> = report.documents.iter().map(|part| part.pages).collect();
    assert_eq!(pages, [2, 2, 1]);
    assert!(report.documents[0].path.ends_with("five-p1-2.pdf"));
    assert!(report.documents[1].path.ends_with("five-p3-4.pdf"));
    assert!(report.documents[2].path.ends_with("five-p5.pdf"));

    for part in &report.documents {
        let inspected = inspect(Path::new(&part.path)).expect("inspect split part");
        assert_eq!(inspected.pages, part.pages);
    }
    let first_text = extract_text(Path::new(&report.documents[0].path)).expect("extract first chunk");
    assert!(first_text.markdown.contains("One") && first_text.markdown.contains("Two"));
    assert!(!first_text.markdown.contains("Three"));
}

#[test]
fn png_and_tiff_predictors_decode_xref_streams() {
    let workspace = Workspace::new();
    for (name, predictor) in [("png.pdf", Predictor::Png), ("tiff.pdf", Predictor::Tiff)] {
        let mut spec = ModernPdf::new("Predictor", &["Predictor text"]);
        spec.predictor = predictor;
        let path = workspace.write(name, &build_modern_pdf(&spec));
        let report = inspect(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(report.pages, 1, "{name}");
        assert_eq!(report.metadata.title.as_deref(), Some("Predictor"), "{name}");
        let extracted = extract_text(&path).expect("extract predictor PDF");
        assert!(extracted.markdown.contains("Predictor text"), "{name}");
    }
}

#[test]
fn truncated_modern_input_reports_an_error_without_panicking() {
    let workspace = Workspace::new();
    let bytes = build_modern_pdf(&ModernPdf::new("Broken", &["Broken body"]));
    let truncated = workspace.write("truncated.pdf", &bytes[..bytes.len() / 2]);

    let error = inspect(&truncated).expect_err("truncated input must fail");
    assert_eq!(error.code(), "format/invalid");
    let error = extract_text(&truncated).expect_err("truncated input must fail");
    assert_eq!(error.code(), "format/invalid");
}

#[test]
fn corrupt_flate_xref_stream_reports_an_error_without_panicking() {
    let workspace = Workspace::new();
    let mut bytes = build_modern_pdf(&ModernPdf::new("Corrupt", &["Corrupt body"]));
    let marker = find_sub(&bytes, b"/Type /XRef").expect("xref marker");
    let stream = find_sub(&bytes[marker..], b"stream\n").expect("xref stream start") + marker + b"stream\n".len();
    let end = find_sub(&bytes[stream..], b"\nendstream").expect("xref stream end") + stream;
    bytes[stream..end].fill(0);

    let path = workspace.write("corrupt-xref.pdf", &bytes);
    let error = inspect(&path).expect_err("corrupt xref stream must fail");
    assert_eq!(error.code(), "format/invalid");
}

fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[test]
fn encrypted_xref_stream_is_still_rejected() {
    let workspace = Workspace::new();
    let mut spec = ModernPdf::new("Encrypted modern", &["Secret"]);
    spec.encrypted = true;
    let path = workspace.write("encrypted-modern.pdf", &build_modern_pdf(&spec));

    let error = inspect(&path).expect_err("encrypted input must fail");
    assert_eq!(error.code(), "format/unsupported");
}

#[test]
fn unsupported_image_filter_in_a_content_stream_is_reported_as_a_loss() {
    let workspace = Workspace::new();
    let path = workspace.path("dct.pdf");
    let bytes = classic_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>",
        "<< /Length 5 /Filter /DCTDecode >>\nstream\nabcde\nendstream",
    ]);
    fs::write(&path, bytes).expect("write DCT PDF");

    let report = inspect(&path).expect("inspect DCT PDF");
    assert_eq!(report.pages, 1);
    let extracted = extract_text(&path).expect("extract DCT PDF");
    assert_eq!(extracted.losses.len(), 1);
    assert_eq!(extracted.losses[0].code, "content/filter-unsupported");
    assert!(extracted.losses[0].message.contains("DCTDecode"), "{:?}", extracted.losses[0].message);
}

fn classic_pdf(objects: &[&str]) -> Vec<u8> {
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

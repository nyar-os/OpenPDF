use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use data_compression::{CodecLimits, DeflateLevel, Zlib};
use open_pdf_core::{
    Bookmark, CreateRequest, DocumentArrange, FormValue, MetadataPatch, StampRequest, arrange, capabilities, clear_metadata, create,
    delete_pages,
    duplicate_pages,
    extract_images, extract_images_pages, extract_text, extract_text_bytes, extract_text_pages, fill_form, inspect, inspect_bytes,
    insert_blank_pages, insert_pages,
    merge, merge_selected, metadata, move_pages, outline, replace_pages, reverse_pages, rotate_pages, search, search_pages,
    set_bookmarks,
    set_metadata,
    split_parts,
    set_page_boxes, split, stamp,
};
use serde_json::json;

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
            ..CreateRequest::default()
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
fn inspect_bytes_reports_a_document_without_a_path() {
    let workspace = Workspace::new();
    let path = workspace.create("memory.pdf", Some("In memory"), &["Alpha"]);
    let bytes = fs::read(&path).expect("read created PDF");

    let report = inspect_bytes(&bytes).expect("inspect bytes");
    assert_eq!(report.label, "<memory>");
    assert_eq!(report.version, "1.7");
    assert_eq!(report.pages, 1);
    assert_eq!(report.metadata.title.as_deref(), Some("In memory"));
}

#[test]
fn extract_text_bytes_reads_a_document_without_a_path() {
    let workspace = Workspace::new();
    let path = workspace.create("memory.pdf", None, &["Alpha", "Beta"]);
    let bytes = fs::read(&path).expect("read created PDF");

    let report = extract_text_bytes(&bytes).expect("extract bytes");
    assert_eq!(report.pages, 2);
    assert!(report.markdown.contains("Alpha"), "page one text: {}", report.markdown);
    assert!(report.markdown.contains("Beta"), "page two text: {}", report.markdown);
}

#[test]
fn extract_text_pages_keeps_only_the_requested_pages_in_order() {
    let workspace = Workspace::new();
    let path = workspace.create("select.pdf", None, &["Alpha", "Beta", "Gamma"]);

    let report = extract_text_pages(&path, "3,1").expect("extract selected pages");
    assert_eq!(report["pages"], json!(2));
    assert_eq!(report["sourcePages"], json!(3));
    assert_eq!(report["selected"], json!([3, 1]));
    let markdown = report["markdown"].as_str().expect("markdown");
    let gamma = markdown.find("Gamma").expect("Gamma is selected");
    let alpha = markdown.find("Alpha").expect("Alpha is selected");
    assert!(gamma < alpha, "requested order is kept: {markdown}");
    assert!(!markdown.contains("Beta"), "unselected pages are dropped: {markdown}");

    let whole = extract_text_pages(&path, "1-3").expect("extract every page");
    assert_eq!(whole["pages"], json!(3));
    assert_eq!(whole["selected"], json!([1, 2, 3]));
    let text = whole["markdown"].as_str().expect("markdown");
    assert!(text.contains("Beta"));

    let error = extract_text_pages(&path, "4").expect_err("page is outside the document");
    assert_eq!(error.code(), "session/argument-invalid");
    assert!(error.message().contains("3-page document"), "unexpected message: {}", error.message());
}

#[test]
fn search_reports_line_scoped_matches_and_honours_the_limit() {
    let workspace = Workspace::new();
    let path = workspace.create("search.pdf", None, &["Alpha invoice one", "Beta invoice two"]);

    let report = search(&path, "invoice", None, true, false, 0).expect("search");
    let insensitive = search(&path, "INVOICE", None, false, false, 0).expect("case-insensitive search");
    assert_eq!(insensitive["hits"].as_array().expect("hits").len(), report["hits"].as_array().expect("hits").len());
    assert_eq!(insensitive["matchCase"], json!(false));
    assert!(insensitive["hits"][0]["match"].as_str().is_some_and(|text| text.eq_ignore_ascii_case("invoice")));
    assert_eq!(report["query"], json!("invoice"));
    assert_eq!(report["truncated"], json!(false));
    let hits = report["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), 2);
    for hit in hits {
        let text = hit["text"].as_str().expect("line text");
        let start = hit["start"].as_u64().expect("start") as usize;
        assert_eq!(hit["match"], json!("invoice"));
        assert_eq!(&text[start..start + "invoice".len()], "invoice");
        assert!(hit["line"].as_u64().expect("line") >= 1);
        assert!(hit["column"].as_u64().expect("column") >= 1);
    }

    let limited = search(&path, "invoice", Some(1), true, false, 0).expect("limited search");
    assert_eq!(limited["hits"].as_array().expect("hits").len(), 1);
    assert_eq!(limited["truncated"], json!(true));

    let whole = search(&path, "invoice", None, true, true, 0).expect("whole-word search");
    assert_eq!(whole["wholeWord"], json!(true));
    assert_eq!(whole["hits"].as_array().expect("hits").len(), 2);
    let partial = search(&path, "invo", None, true, true, 0).expect("whole-word rejects a fragment");
    assert!(partial["hits"].as_array().expect("hits").is_empty());

    let paged = search(&path, "invoice", Some(1), true, false, 1).expect("skipped search");
    assert_eq!(paged["skip"], json!(1));
    let paged_hits = paged["hits"].as_array().expect("hits");
    assert_eq!(paged_hits.len(), 1);
    assert_eq!(paged_hits[0]["line"], report["hits"][1]["line"], "the first match was discarded");
    let beyond = search(&path, "invoice", None, true, false, 5).expect("skip past the end");
    assert!(beyond["hits"].as_array().expect("hits").is_empty());

    let empty = search(&path, "   ", None, true, false, 0).expect_err("empty query");
    assert_eq!(empty.code(), "session/argument-invalid");
}

#[test]
fn search_pages_only_scans_the_selected_pages() {
    let workspace = Workspace::new();
    let path = workspace.create("search-pages.pdf", None, &["Alpha invoice one", "Beta invoice two", "Gamma invoice three"]);

    let report = search_pages(&path, "invoice", None, "3,1", true, false, 0).expect("search selected pages");
    assert_eq!(report["sourcePages"], json!(3));
    assert_eq!(report["selected"], json!([3, 1]));
    assert_eq!(report["pages"], json!(2));
    let hits = report["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|hit| hit["text"].as_str().unwrap_or_default().contains("Alpha")));
    assert!(hits.iter().any(|hit| hit["text"].as_str().unwrap_or_default().contains("Gamma")));
    assert!(!hits.iter().any(|hit| hit["text"].as_str().unwrap_or_default().contains("Beta")));

    let whole = search_pages(&path, "invoice", Some(1), "1-3", true, false, 0).expect("search every page");
    assert_eq!(whole["sourcePages"], json!(3));
    assert_eq!(whole["selected"], json!([1, 2, 3]));
    assert_eq!(whole["hits"].as_array().expect("hits").len(), 1);
    assert_eq!(whole["truncated"], json!(true));

    let empty = search_pages(&path, " ", None, "1", true, false, 0).expect_err("empty query");
    assert_eq!(empty.code(), "session/argument-invalid");

    let error = search_pages(&path, "invoice", None, "4", true, false, 0).expect_err("page is outside the document");
    assert_eq!(error.code(), "session/argument-invalid");
}

#[test]
fn extract_text_pages_rejects_malformed_ranges() {
    let workspace = Workspace::new();
    let path = workspace.create("ranges.pdf", None, &["Alpha", "Beta"]);

    for ranges in ["", "0", "2-1", "1,,2", "1,1", "a", "1-"] {
        let error = extract_text_pages(&path, ranges).expect_err("invalid ranges");
        assert_eq!(error.code(), "session/argument-invalid", "ranges `{ranges}`");
    }
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
fn split_parts_balances_page_counts_with_earlier_remainders() {
    let workspace = Workspace::new();
    let path = workspace.create("seven.pdf", None, &["One", "Two", "Three", "Four", "Five", "Six", "Seven"]);
    let out_dir = workspace.path("balanced");

    let report = split_parts(&path, 3, &out_dir).expect("split into parts");
    assert_eq!(report["parts"], json!(3));
    assert_eq!(report["pages"], json!(7));
    let documents = report["documents"].as_array().expect("documents");
    let pages: Vec<u64> = documents.iter().map(|part| part["pages"].as_u64().expect("pages")).collect();
    assert_eq!(pages, vec![3, 2, 2], "earlier files take the remainder");
    for part in documents {
        let inspected = inspect(Path::new(part["path"].as_str().expect("path"))).expect("inspect part");
        assert_eq!(inspected.pages as u64, part["pages"].as_u64().expect("pages"));
    }

    assert_invalid(split_parts(&path, 0, &workspace.path("empty")), "session/argument-invalid");
    assert_invalid(split_parts(&path, 8, &workspace.path("many")), "session/argument-invalid");
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

fn image_pages_pdf() -> Vec<u8> {
    fn gray(width: u32, height: u32, value: u8) -> Vec<u8> {
        let data = Zlib::encode(&vec![value; (width * height) as usize], DeflateLevel::Default);
        let mut stream = format!(
            "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /BitsPerComponent 8 \
             /ColorSpace /DeviceGray /Filter /FlateDecode /Length {} >>\nstream\n",
            data.len()
        )
        .into_bytes();
        stream.extend_from_slice(&data);
        stream.extend_from_slice(b"\nendstream");
        stream
    }

    assemble_bytes(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << /XObject << /Im 5 0 R >> >> >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << /XObject << /Im 6 0 R >> >> >>".to_vec(),
        gray(1, 1, 10),
        gray(2, 1, 200),
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
fn extract_images_pages_keeps_only_selected_pages_and_remaps_their_numbers() {
    let workspace = Workspace::new();
    let path = workspace.write("pages.pdf", &image_pages_pdf());
    let out_dir = workspace.path("selected-images");

    let selected = extract_images_pages(&path, &out_dir, "2").expect("extract selected images");
    let images = selected.as_array().expect("image array");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["page"], json!(2));
    let written = images[0]["path"].as_str().expect("written path");
    assert!(written.ends_with("pages-p2-img1.png"), "{written}");
    let (width, height, _, _) = png_dimensions(&fs::read(written).expect("read selected PNG"));
    assert_eq!((width, height), (2, 1));

    let whole = extract_images_pages(&path, &workspace.path("all-images"), "1-2").expect("extract every image");
    let images = whole.as_array().expect("image array");
    assert_eq!(images.len(), 2);
    assert_eq!(images[0]["page"], json!(1));
    assert_eq!(images[1]["page"], json!(2));

    let error = extract_images_pages(&path, &workspace.path("bad-images"), "3").expect_err("page outside the document");
    assert_eq!(error.code(), "session/argument-invalid");
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
    assert_eq!(matrix["operations"]["arrange"], "supported");
    assert_eq!(matrix["operations"]["setMetadata"], "supported");
    assert_eq!(matrix["operations"]["fillForm"], "supported");
    assert_eq!(matrix["operations"]["deletePages"], "supported");
    assert_eq!(matrix["operations"]["insertPages"], "supported");
    assert_eq!(matrix["operations"]["insertBlankPages"], "supported");
    assert_eq!(matrix["operations"]["replacePages"], "supported");
    assert_eq!(matrix["operations"]["reversePages"], "supported");
    assert_eq!(matrix["operations"]["rotatePages"], "supported");
    assert_eq!(matrix["operations"]["setPageBoxes"], "supported");
    assert_eq!(matrix["operations"]["metadata"], "supported");
    let color_spaces = &matrix["formats"]["pdf"]["image"]["colorSpaces"];
    for color_space in ["deviceGray", "deviceRgb", "deviceCmyk", "indexed"] {
        assert_eq!(color_spaces[color_space], "supported", "{color_space}");
    }
}

#[test]
fn arrange_removes_then_orders_and_rotates() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two", "Three"]);
    let out = workspace.path("arranged.pdf");

    let report = arrange(&path, &out, &DocumentArrange { order: Some(vec![3, 1]), rotate: Some(90), remove: Some(vec![2]) })
        .expect("arrange");
    assert_eq!(report.pages, 2);

    let inspected = inspect(&out).expect("inspect arranged");
    assert_eq!(inspected.pages, 2);
    assert_eq!(inspected.page_sizes[0].rotation, 90);
    assert_eq!(inspected.page_sizes[1].rotation, 90);

    let extracted = extract_text(&out).expect("extract arranged");
    let three = extracted.markdown.find("Three").expect("Three");
    let one = extracted.markdown.find("One").expect("One");
    assert!(three < one, "pages are out of order: {:?}", extracted.markdown);
    assert!(!extracted.markdown.contains("Two"), "{:?}", extracted.markdown);
}

#[test]
fn arrange_adds_rotation_to_the_existing_value_and_normalizes() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "rotated.pdf",
        &assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] /Rotate 90 >>",
            "<< /Type /Page /Parent 2 0 R >>",
        ]),
    );

    let quarter = workspace.path("quarter.pdf");
    arrange(&path, &quarter, &DocumentArrange { rotate: Some(180), ..DocumentArrange::default() }).expect("arrange");
    assert_eq!(inspect(&quarter).expect("inspect").page_sizes[0].rotation, 270);

    let full = workspace.path("full.pdf");
    arrange(&path, &full, &DocumentArrange { rotate: Some(270), ..DocumentArrange::default() }).expect("arrange");
    assert_eq!(inspect(&full).expect("inspect").page_sizes[0].rotation, 0);
    let bytes = fs::read(&full).expect("read arranged");
    assert!(!String::from_utf8_lossy(&bytes).contains("/Rotate"), "a zero rotation must be omitted");
}

#[test]
fn arrange_rejects_invalid_indices_duplicates_and_empty_results() {
    let workspace = Workspace::new();
    let path = workspace.create("three.pdf", None, &["One", "Two", "Three"]);
    let out = workspace.path("invalid.pdf");

    let cases = [
        DocumentArrange { order: Some(vec![0]), ..DocumentArrange::default() },
        DocumentArrange { order: Some(vec![4]), ..DocumentArrange::default() },
        DocumentArrange { order: Some(vec![1, 1, 2]), ..DocumentArrange::default() },
        DocumentArrange { order: Some(vec![1, 2]), ..DocumentArrange::default() },
        DocumentArrange { order: Some(vec![2]), remove: Some(vec![2]), ..DocumentArrange::default() },
        DocumentArrange { remove: Some(vec![4]), ..DocumentArrange::default() },
        DocumentArrange { remove: Some(vec![1, 1]), ..DocumentArrange::default() },
        DocumentArrange { remove: Some(vec![1, 2, 3]), ..DocumentArrange::default() },
        DocumentArrange { rotate: Some(45), ..DocumentArrange::default() },
    ];
    for ops in &cases {
        assert_invalid(arrange(&path, &out, ops), "session/argument-invalid");
    }
}

#[test]
fn duplicate_pages_repeats_the_selection_in_place() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two", "Three"]);
    let out = workspace.path("doubled.pdf");

    let report = duplicate_pages(&path, "2", 2, &out, false).expect("duplicate pages");
    assert_eq!(report["pages"], json!(4));
    assert_eq!(report["sourcePages"], json!(3));
    assert_eq!(report["duplicated"], json!([2]));
    assert_eq!(report["times"], json!(2));

    let inspected = inspect(&out).expect("inspect duplicated");
    assert_eq!(inspected.pages, 4);
    let extracted = extract_text(&out).expect("extract duplicated");
    let text = extracted.markdown;
    let one = text.find("One").expect("One");
    let first_two = text.find("Two").expect("first Two");
    let second_two = text.rfind("Two").expect("second Two");
    let three = text.find("Three").expect("Three");
    assert!(one < first_two && first_two < second_two && second_two < three, "unexpected order: {text}");

    assert_invalid(duplicate_pages(&path, "1", 2, &out, false), "io/exists");
    assert_invalid(duplicate_pages(&path, "1", 1, &workspace.path("times.pdf"), true), "session/argument-invalid");
    assert_invalid(duplicate_pages(&path, "9", 2, &workspace.path("range.pdf"), true), "session/argument-invalid");
}

#[test]
fn insert_pages_places_the_other_document_after_the_selected_page() {
    let workspace = Workspace::new();
    let target = workspace.create("deck.pdf", None, &["One", "Two", "Three"]);
    let insert = workspace.create("extra.pdf", None, &["Alpha", "Beta"]);
    let out = workspace.path("inserted.pdf");

    let report = insert_pages(&target, &insert, None, 1, &out, false).expect("insert pages");
    assert_eq!(report["pages"], json!(5));
    assert_eq!(report["sourcePages"], json!(3));
    assert_eq!(report["insertedPages"], json!(2));
    assert_eq!(report["after"], json!(1));
    assert_eq!(report["path"].as_str(), Some(out.display().to_string().as_str()));

    let text = extract_text(&out).expect("extract inserted").markdown;
    let one = text.find("One").expect("One");
    let alpha = text.find("Alpha").expect("Alpha");
    let beta = text.find("Beta").expect("Beta");
    let two = text.find("Two").expect("Two");
    let three = text.find("Three").expect("Three");
    assert!(one < alpha && alpha < beta && beta < two && two < three, "unexpected order: {text}");

    let front = workspace.path("front.pdf");
    insert_pages(&target, &insert, None, 0, &front, false).expect("insert before the first page");
    let text = extract_text(&front).expect("extract front").markdown;
    assert!(text.find("Alpha").expect("Alpha") < text.find("One").expect("One"), "unexpected order: {text}");

    assert_invalid(insert_pages(&target, &insert, None, 4, &out, false), "session/argument-invalid");
    assert_invalid(insert_pages(&target, &insert, None, 1, &out, false), "io/exists");

    let selected_out = workspace.path("selected.pdf");
    let report = insert_pages(&target, &insert, Some("2"), 1, &selected_out, false).expect("insert one selected page");
    assert_eq!(report["pages"], json!(4));
    assert_eq!(report["insertedPages"], json!(1));
    assert_eq!(report["sourceInsertPages"], json!(2));
    assert_eq!(report["selectedPages"], json!([2]));
    let text = extract_text(&selected_out).expect("extract selected").markdown;
    assert!(text.contains("Beta"), "the chosen page goes in: {text}");
    assert!(!text.contains("Alpha"), "the unselected page stays out: {text}");
    assert_invalid(insert_pages(&target, &insert, Some("3"), 1, &workspace.path("bad.pdf"), false), "session/argument-invalid");
}

#[test]
fn insert_blank_pages_keeps_order_and_geometry() {
    let workspace = Workspace::new();
    let target = workspace.create("deck.pdf", None, &["One", "Two"]);
    let out = workspace.path("blank.pdf");

    let report = insert_blank_pages(&target, 1, 2, None, &out, false).expect("insert blanks");
    assert_eq!(report["pages"], json!(4));
    assert_eq!(report["sourcePages"], json!(2));
    assert_eq!(report["insertedPages"], json!(2));
    assert_eq!(report["after"], json!(1));

    let inspected = inspect(&out).expect("inspect inserted");
    assert_eq!(inspected.pages, 4);
    for page in &inspected.page_sizes {
        assert!(
            (page.width_pt - 612.0).abs() < 1.0 && (page.height_pt - 792.0).abs() < 1.0,
            "blanks keep the source geometry: {page:?}"
        );
    }
    let text = extract_text(&out).expect("extract inserted").markdown;
    assert!(text.find("One").expect("One") < text.find("Two").expect("Two"), "page order changed: {text}");

    let a4 = workspace.path("a4.pdf");
    insert_blank_pages(&target, 0, 1, Some("a4"), &a4, false).expect("insert an A4 blank first");
    let inspected = inspect(&a4).expect("inspect a4");
    assert_eq!(inspected.pages, 3);
    assert!((inspected.page_sizes[0].width_pt - 595.276).abs() < 1.0, "the explicit size wins: {inspected:?}");

    assert_invalid(insert_blank_pages(&target, 3, 1, None, &out, true), "session/argument-invalid");
    assert_invalid(insert_blank_pages(&target, 0, 0, None, &workspace.path("zero.pdf"), true), "session/argument-invalid");
    assert_invalid(insert_blank_pages(&target, 0, 101, None, &workspace.path("many.pdf"), true), "session/argument-invalid");
    assert_invalid(insert_blank_pages(&target, 1, 1, None, &out, false), "io/exists");
}

#[test]
fn replace_pages_splices_the_replacement_in_place() {
    let workspace = Workspace::new();
    let target = workspace.create("deck.pdf", None, &["One", "Two", "Three", "Four"]);
    let insert = workspace.create("extra.pdf", None, &["Alpha", "Beta"]);
    let out = workspace.path("replaced.pdf");

    let report = replace_pages(&target, "2-3", &insert, None, &out, false).expect("replace pages");
    assert_eq!(report["pages"], json!(4));
    assert_eq!(report["sourcePages"], json!(4));
    assert_eq!(report["replaced"], json!([2, 3]));
    assert_eq!(report["replacementPages"], json!(2));
    assert_eq!(report["sourceInsertPages"], json!(2));
    assert_eq!(report["selectedPages"], json!([1, 2]));

    let text = extract_text(&out).expect("extract replaced").markdown;
    let one = text.find("One").expect("One");
    let alpha = text.find("Alpha").expect("Alpha");
    let beta = text.find("Beta").expect("Beta");
    let four = text.find("Four").expect("Four");
    assert!(one < alpha && alpha < beta && beta < four, "unexpected order: {text}");
    assert!(!text.contains("Two") && !text.contains("Three"), "removed pages survived: {text}");

    // Selecting source pages chooses only those pages and keeps their order.
    let chosen = workspace.path("chosen.pdf");
    let report = replace_pages(&target, "2-3", &insert, Some("2"), &chosen, false).expect("replace with one source page");
    assert_eq!(report["replacementPages"], json!(1));
    assert_eq!(report["selectedPages"], json!([2]));
    let text = extract_text(&chosen).expect("extract chosen").markdown;
    assert!(!text.contains("Alpha") && text.contains("Beta"), "only the chosen source page is used: {text}");

    let tail = workspace.path("tail.pdf");
    replace_pages(&target, "4", &insert, None, &tail, false).expect("replace the tail");
    let text = extract_text(&tail).expect("extract tail").markdown;
    assert!(text.find("Three").expect("Three") < text.find("Alpha").expect("Alpha"), "unexpected order: {text}");

    assert_invalid(
        replace_pages(&target, "9", &insert, None, &workspace.path("range.pdf"), true),
        "session/argument-invalid",
    );
    assert_invalid(replace_pages(&target, "1", &insert, Some("3"), &workspace.path("bad-source.pdf"), true), "session/argument-invalid");
    assert_invalid(replace_pages(&target, "1", &insert, None, &out, false), "io/exists");

    let empty = workspace.write("empty.pdf", &assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [] /Count 0 >>",
    ]));
    assert_invalid(replace_pages(&target, "1", &empty, None, &workspace.path("empty-out.pdf"), true), "session/argument-invalid");
}

#[test]
fn reverse_pages_flips_the_document_order() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two", "Three"]);
    let out = workspace.path("reversed.pdf");

    let report = reverse_pages(&path, &out, false, None).expect("reverse pages");
    assert_eq!(report["pages"], json!(3));
    assert_eq!(report["sourcePages"], json!(3));
    assert_eq!(report["reversed"], json!([1, 2, 3]));
    assert_eq!(report["path"].as_str(), Some(out.display().to_string().as_str()));

    let text = extract_text(&out).expect("extract reversed").markdown;
    let three = text.find("Three").expect("Three");
    let two = text.find("Two").expect("Two");
    let one = text.find("One").expect("One");
    assert!(three < two && two < one, "unexpected order: {text}");

    // Only the selected pages trade places; the unselected page keeps its spot.
    let middle = workspace.path("middle.pdf");
    let report = reverse_pages(&path, &middle, false, Some("1,2")).expect("reverse the first two pages");
    assert_eq!(report["reversed"], json!([1, 2]));
    let text = extract_text(&middle).expect("extract middle").markdown;
    let two = text.find("Two").expect("Two");
    let one = text.find("One").expect("One");
    let three = text.find("Three").expect("Three");
    assert!(two < one && one < three, "unexpected order: {text}");

    assert_invalid(reverse_pages(&path, &middle, true, Some("9")), "session/argument-invalid");
    assert_invalid(reverse_pages(&path, &out, false, None), "io/exists");
}

#[test]
fn move_pages_relocates_a_selection_after_one_anchor() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two", "Three", "Four"]);
    let out = workspace.path("moved.pdf");

    let report = move_pages(&path, "1", 3, &out, false).expect("move the first page after the third");
    assert_eq!(report["pages"], json!(4));
    assert_eq!(report["sourcePages"], json!(4));
    assert_eq!(report["moved"], json!([1]));
    assert_eq!(report["after"], json!(3));
    let text = extract_text(&out).expect("extract moved").markdown;
    let positions = ["One", "Two", "Three", "Four"].map(|page| text.find(page).expect(page));
    let [one, two, three, four] = positions;
    assert!(two < three && three < one && one < four, "unexpected order: {text}");

    let front = workspace.path("front.pdf");
    move_pages(&path, "3-4", 0, &front, false).expect("move the tail to the front");
    let text = extract_text(&front).expect("extract front").markdown;
    let positions = ["One", "Two", "Three", "Four"].map(|page| text.find(page).expect(page));
    let [one, two, three, four] = positions;
    assert!(three < four && four < one && one < two, "unexpected order: {text}");

    assert_invalid(move_pages(&path, "2", 9, &workspace.path("bad.pdf"), true), "session/argument-invalid");
    assert_invalid(move_pages(&path, "2-3", 2, &workspace.path("bad-anchor.pdf"), true), "session/argument-invalid");
    assert_invalid(move_pages(&path, "1", 3, &out, false), "io/exists");
}

#[test]
fn metadata_reads_the_information_dictionary() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", Some("Quarterly deck"), &["One", "Two"]);

    let report = metadata(&path).expect("read metadata");
    assert_eq!(report["path"].as_str(), Some(path.display().to_string().as_str()));
    assert_eq!(report["pages"], json!(2));
    assert_eq!(report["metadata"]["title"].as_str(), Some("Quarterly deck"));
    assert_eq!(report["metadata"]["producer"].as_str(), Some("OpenPDF"));

    let broken = workspace.write("broken-metadata.pdf", b"not a PDF");
    assert_invalid(metadata(&broken), "format/invalid");
}

#[test]
fn arrange_on_corrupt_input_reports_format_invalid_without_panicking() {
    let workspace = Workspace::new();
    let broken = workspace.write("broken-arrange.pdf", b"not a PDF");
    assert_invalid(arrange(&broken, &workspace.path("out.pdf"), &DocumentArrange::default()), "format/invalid");
}

#[test]
fn delete_pages_writes_the_surviving_pages() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two", "Three"]);
    let out = workspace.path("kept.pdf");

    let report = delete_pages(&path, "2", &out, false).expect("delete pages");
    assert_eq!(report.pages, 2);
    assert_eq!(report.removed, 1);
    assert!(report.bytes > 0);

    let inspected = inspect(&out).expect("inspect output");
    assert_eq!(inspected.pages, 2);
    let extracted = extract_text(&out).expect("extract output");
    assert!(extracted.markdown.contains("One"), "{:?}", extracted.markdown);
    assert!(extracted.markdown.contains("Three"), "{:?}", extracted.markdown);
    assert!(!extracted.markdown.contains("Two"), "{:?}", extracted.markdown);

    assert_invalid(delete_pages(&path, "1-3", &out, true), "session/argument-invalid");
    assert_invalid(delete_pages(&path, "9", &out, true), "session/argument-invalid");
}

#[test]
fn rotate_pages_rotates_only_the_selected_pages() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two"]);
    let out = workspace.path("rotated.pdf");

    let report = rotate_pages(&path, "2", 180, &out, false).expect("rotate pages");
    assert_eq!(report.pages, 2);
    assert_eq!(report.rotated, 1);
    assert!(report.bytes > 0);

    let inspected = inspect(&out).expect("inspect rotated");
    assert_eq!(inspected.pages, 2);
    assert_eq!(inspected.page_sizes[0].rotation, 0);
    assert_eq!(inspected.page_sizes[1].rotation, 180);

    assert_invalid(rotate_pages(&path, "1", 45, &out, true), "session/argument-invalid");
}

#[test]
fn set_page_boxes_writes_the_crop_box_of_the_selected_pages() {
    let workspace = Workspace::new();
    let path = workspace.create("deck.pdf", None, &["One", "Two"]);
    let out = workspace.path("cropped.pdf");

    let report = set_page_boxes(&path, "1", [10.0, 20.0, 300.0, 400.0], &out, false).expect("set page boxes");
    assert_eq!(report.pages, 2);
    assert_eq!(report.cropped, 1);
    assert!(report.bytes > 0);

    let inspected = inspect(&out).expect("inspect cropped");
    assert_eq!(inspected.pages, 2);
    assert_eq!(inspected.page_sizes[0].width_pt, 612.0);
    assert_eq!(inspected.page_sizes[0].height_pt, 792.0);

    let bytes = fs::read(&out).expect("read cropped");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/CropBox [10 20 300 400]"), "crop box is missing");

    assert_invalid(set_page_boxes(&path, "1", [300.0, 20.0, 10.0, 400.0], &out, true), "session/argument-invalid");
    assert_invalid(set_page_boxes(&path, "1", [0.0, 0.0, 700.0, 400.0], &out, true), "session/argument-invalid");
}

#[test]
fn set_metadata_overrides_preserves_and_deletes_fields() {
    let workspace = Workspace::new();
    let path = workspace.create("source.pdf", Some("Original"), &["One", "Two"]);

    let out = workspace.path("patched.pdf");
    let report = set_metadata(
        &path,
        &out,
        &MetadataPatch {
            title: Some("Rewritten".to_string()),
            author: Some("Alice".to_string()),
            producer: Some(String::new()),
            ..MetadataPatch::default()
        },
    )
    .expect("set metadata");
    assert_eq!(report.pages, 2);
    assert_eq!(report.metadata.title.as_deref(), Some("Rewritten"));
    assert_eq!(report.metadata.author.as_deref(), Some("Alice"));
    assert_eq!(report.metadata.producer, None);

    let inspected = inspect(&out).expect("inspect patched");
    assert_eq!(inspected.pages, 2);
    assert_eq!(inspected.metadata.title.as_deref(), Some("Rewritten"));
    assert_eq!(inspected.metadata.author.as_deref(), Some("Alice"));
    assert_eq!(inspected.metadata.producer, None);
    assert_eq!(inspected.metadata.subject, None);

    let kept = workspace.path("kept.pdf");
    let report = set_metadata(&path, &kept, &MetadataPatch { author: Some("Bob".to_string()), ..MetadataPatch::default() })
        .expect("partial metadata");
    assert_eq!(report.metadata.title.as_deref(), Some("Original"));
    assert_eq!(report.metadata.producer.as_deref(), Some("OpenPDF"));
    assert_eq!(report.metadata.author.as_deref(), Some("Bob"));
}

#[test]
fn set_metadata_on_corrupt_input_reports_format_invalid_without_panicking() {
    let workspace = Workspace::new();
    let broken = workspace.write("broken-metadata.pdf", b"%PDF-1.7\nnot really");
    assert_invalid(set_metadata(&broken, &workspace.path("out.pdf"), &MetadataPatch::default()), "format/invalid");
}

#[test]
fn create_applies_geometry_font_alignment_metadata_and_page_numbers() {
    let workspace = Workspace::new();
    let out = workspace.path("styled.pdf");
    let report = create(&CreateRequest {
        out: out.clone(),
        title: Some("Styled".to_string()),
        pages: vec!["Centered body text that should wrap when the column is narrow enough".to_string()],
        overwrite: true,
        page_size: Some("a4".to_string()),
        font: Some("times".to_string()),
        font_size: Some(18.0),
        margin_pt: Some(54.0),
        align: Some("center".to_string()),
        author: Some("Alice".to_string()),
        subject: Some("Styling".to_string()),
        keywords: Some("pdf, style".to_string()),
        page_numbers: Some(true),
        ..CreateRequest::default()
    })
    .expect("styled create");

    assert_eq!(report.pages, 1);
    let inspected = inspect(&out).expect("inspect styled");
    assert_eq!(inspected.metadata.title.as_deref(), Some("Styled"));
    assert_eq!(inspected.metadata.author.as_deref(), Some("Alice"));
    assert_eq!(inspected.metadata.subject.as_deref(), Some("Styling"));
    assert_eq!(inspected.metadata.keywords.as_deref(), Some("pdf, style"));
    assert!((inspected.page_sizes[0].width_pt - 595.28).abs() < 0.01);
    assert!((inspected.page_sizes[0].height_pt - 841.89).abs() < 0.01);

    let text = extract_text(&out).expect("extract styled");
    assert!(text.markdown.contains("Centered body text"), "{:?}", text.markdown);
    assert!(text.markdown.contains("1 / 1"), "page number footer: {:?}", text.markdown);
}

#[test]
fn create_paginates_long_page_text() {
    let workspace = Workspace::new();
    let out = workspace.path("long.pdf");
    let lines = (1..=80).map(|index| format!("line {index}")).collect::<Vec<_>>();
    let report = create(&CreateRequest {
        out: out.clone(),
        pages: vec![lines.join("\n")],
        overwrite: true,
        page_size: Some("a4".to_string()),
        ..CreateRequest::default()
    })
    .expect("paginated create");

    assert!(report.pages > 1, "80 lines at 12pt need more than one page");
    assert_eq!(inspect(&out).expect("inspect long").pages, report.pages);
}

#[test]
fn create_rejects_invalid_geometry_and_style() {
    let workspace = Workspace::new();
    let base = CreateRequest { out: workspace.path("bad.pdf"), pages: vec!["x".to_string()], overwrite: true, ..CreateRequest::default() };

    assert_invalid(create(&CreateRequest { page_size: Some("huge".to_string()), ..base.clone() }), "session/argument-invalid");
    assert_invalid(create(&CreateRequest { align: Some("middle".to_string()), ..base.clone() }), "session/argument-invalid");
    assert_invalid(create(&CreateRequest { font: Some("comic".to_string()), ..base.clone() }), "session/argument-invalid");
    assert_invalid(create(&CreateRequest { font_size: Some(2.0), ..base.clone() }), "session/argument-invalid");
    assert_invalid(create(&CreateRequest { margin_pt: Some(2.0), ..base }), "session/argument-invalid");
}

#[test]
fn stamp_marks_only_the_requested_pages_and_keeps_content() {
    let workspace = Workspace::new();
    let path = workspace.create("source.pdf", None, &["Alpha", "Beta"]);
    let out = workspace.path("stamped.pdf");
    let report = stamp(
        &path,
        &out,
        &StampRequest {
            text: "DRAFT {page}/{pages}".to_string(),
            position: Some("bottom".to_string()),
            pages: Some("2".to_string()),
            ..StampRequest::default()
        },
    )
    .expect("stamp");

    assert_eq!(report.pages, 2);
    assert_eq!(report.pages_stamped, 1);
    let extracted = extract_text(&out).expect("extract stamped");
    assert!(extracted.markdown.contains("Alpha"), "{:?}", extracted.markdown);
    assert!(extracted.markdown.contains("Beta"), "{:?}", extracted.markdown);
    assert_eq!(extracted.markdown.matches("DRAFT 2/2").count(), 1, "{:?}", extracted.markdown);
    assert!(!extracted.markdown.contains("DRAFT 1/2"), "page one stays clean: {:?}", extracted.markdown);
    assert_eq!(inspect(&out).expect("inspect stamped").pages, 2);
}

#[test]
fn stamp_validates_text_opacity_and_ranges() {
    let workspace = Workspace::new();
    let path = workspace.create("source.pdf", None, &["Only"]);
    let out = workspace.path("bad-stamp.pdf");

    assert_invalid(stamp(&path, &out, &StampRequest { text: "   ".to_string(), ..StampRequest::default() }), "session/argument-invalid");
    assert_invalid(
        stamp(&path, &out, &StampRequest { text: "x".to_string(), opacity: Some(2.0), ..StampRequest::default() }),
        "session/argument-invalid",
    );
    assert_invalid(
        stamp(&path, &out, &StampRequest { text: "x".to_string(), pages: Some("9".to_string()), ..StampRequest::default() }),
        "session/argument-invalid",
    );
}

#[test]
fn set_bookmarks_writes_a_readable_outline_tree() {
    let workspace = Workspace::new();
    let path = workspace.create("source.pdf", Some("Original"), &["One", "Two"]);
    let out = workspace.path("outlined.pdf");

    let report = set_bookmarks(
        &path,
        &out,
        &[
            Bookmark { title: "Intro (start)".to_string(), page: 1, depth: None },
            Bookmark { title: "Details\\n".to_string(), page: 2, depth: Some(1) },
            Bookmark { title: "End".to_string(), page: 2, depth: Some(0) },
        ],
    )
    .expect("set bookmarks");
    assert_eq!(report.pages, 2);
    assert_eq!(report.bookmarks, 3);
    assert_eq!(report.path, out.display().to_string());

    let inspected = inspect(&out).expect("inspect bookmarks");
    assert_eq!(inspected.pages, 2);
    assert_eq!(inspected.metadata.title.as_deref(), Some("Original"));
    let entries: Vec<(&str, usize)> =
        inspected.bookmarks.iter().map(|bookmark| (bookmark.title.as_str(), bookmark.depth)).collect();
    assert_eq!(entries, [("Intro (start)", 1), ("Details\\n", 2), ("End", 1)]);

    let bytes = fs::read(&out).expect("read outlined PDF");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/PageMode /UseOutlines"), "page mode is missing");
    assert!(text.contains("/Dest ["), "outline destinations are missing");
    assert!(text.contains("/Fit"), "destination fit type is missing");

    let extracted = extract_text(&out).expect("extract outlined");
    assert!(extracted.markdown.contains("One"), "{:?}", extracted.markdown);
    assert!(extracted.markdown.contains("Two"), "{:?}", extracted.markdown);
}

#[test]
fn outline_reports_entries_counts_and_nesting_depth() {
    let workspace = Workspace::new();
    let path = workspace.create("source.pdf", None, &["One", "Two"]);
    let out = workspace.path("outline-tree.pdf");

    set_bookmarks(
        &path,
        &out,
        &[
            Bookmark { title: "Intro".to_string(), page: 1, depth: None },
            Bookmark { title: "Details".to_string(), page: 2, depth: Some(1) },
            Bookmark { title: "End".to_string(), page: 2, depth: Some(0) },
        ],
    )
    .expect("set bookmarks");

    let report = outline(&out).expect("read outline");
    assert_eq!(report["path"].as_str(), Some(out.display().to_string().as_str()));
    assert_eq!(report["pages"].as_u64(), Some(2));
    assert_eq!(report["count"].as_u64(), Some(3));
    assert_eq!(report["maxDepth"].as_u64(), Some(2));
    let bookmarks = report["bookmarks"].as_array().expect("bookmark list");
    let entries: Vec<(&str, u64)> = bookmarks
        .iter()
        .map(|bookmark| {
            (bookmark["title"].as_str().expect("title"), bookmark["depth"].as_u64().expect("depth"))
        })
        .collect();
    assert_eq!(entries, [("Intro", 1), ("Details", 2), ("End", 1)]);

    let empty = workspace.create("plain.pdf", None, &["Plain"]);
    let plain = outline(&empty).expect("read empty outline");
    assert_eq!(plain["count"].as_u64(), Some(0));
    assert_eq!(plain["maxDepth"].as_u64(), Some(0));
    assert_eq!(plain["bookmarks"].as_array().map(Vec::len), Some(0));
}

#[test]
fn set_bookmarks_rejects_invalid_pages_titles_and_depth() {
    let workspace = Workspace::new();
    let path = workspace.create("source.pdf", None, &["One", "Two"]);
    let out = workspace.path("bad-bookmarks.pdf");

    assert_invalid(
        set_bookmarks(&path, &out, &[Bookmark { title: "Out".to_string(), page: 3, depth: None }]),
        "session/argument-invalid",
    );
    assert_invalid(
        set_bookmarks(&path, &out, &[Bookmark { title: "   ".to_string(), page: 1, depth: None }]),
        "session/argument-invalid",
    );
    assert_invalid(
        set_bookmarks(&path, &out, &[Bookmark { title: "Deep".to_string(), page: 1, depth: Some(64) }]),
        "session/argument-invalid",
    );
    assert_invalid(set_bookmarks(&path, &out, &[]), "session/argument-invalid");
}

fn flate_image(width: u32, height: u32, color_space: &str, bits: u8, decode: &str, data: &[u8]) -> Vec<u8> {
    let encoded = Zlib::encode(data, DeflateLevel::Default);
    let mut stream = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /BitsPerComponent {bits} \
         /ColorSpace {color_space}{decode} /Filter /FlateDecode /Length {} >>\nstream\n",
        encoded.len()
    )
    .into_bytes();
    stream.extend_from_slice(&encoded);
    stream.extend_from_slice(b"\nendstream");
    stream
}

fn color_image_pdf() -> Vec<u8> {
    let mut icc_stream = b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 \
        /ColorSpace [/ICCBased 10 0 R] /Filter /FlateDecode /Length "
        .to_vec();
    let icc = Zlib::encode(&[0u8, 0, 0], DeflateLevel::Default);
    icc_stream.extend_from_slice(icc.len().to_string().as_bytes());
    icc_stream.extend_from_slice(b" >>\nstream\n");
    icc_stream.extend_from_slice(&icc);
    icc_stream.extend_from_slice(b"\nendstream");

    assemble_bytes(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << /XObject << \
          /Cmyk 4 0 R /Indexed 5 0 R /IndexedInvert 6 0 R /BadIndexed 7 0 R /Nibble 8 0 R /Icc 9 0 R >> >> >>"
            .to_vec(),
        flate_image(1, 1, "/DeviceCMYK", 8, "", &[0, 0, 0, 0]),
        flate_image(2, 1, "[/Indexed /DeviceRGB 1 <FF000000FF00>]", 8, "", &[0, 255]),
        flate_image(2, 1, "[/Indexed /DeviceRGB 1 <FF000000FF00>]", 8, " /Decode [1 0]", &[0, 255]),
        flate_image(1, 1, "[/Indexed /DeviceRGB 4 <FF000000FF00>]", 8, "", &[0]),
        flate_image(1, 1, "[/Indexed /DeviceGray 15 <00000000000000000000000000000000>]", 4, "", &[0x10]),
        icc_stream,
        b"<< /N 3 /Length 0 >>\nstream\n\nendstream".to_vec(),
    ])
}

fn png_scanlines(bytes: &[u8]) -> Vec<u8> {
    let mut position = 8usize;
    while position + 8 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[position..position + 4].try_into().expect("chunk length")) as usize;
        let kind = &bytes[position + 4..position + 8];
        let data = &bytes[position + 8..position + 8 + length];
        if kind == b"IDAT" {
            let limits = CodecLimits::new().with_max_output_bytes(1 << 20);
            return Zlib::decode(data, limits).expect("inflate PNG IDAT").value;
        }
        position += 12 + length;
    }
    panic!("PNG has no IDAT chunk");
}

#[test]
fn extract_images_writes_cmyk_and_indexed_pngs() {
    let workspace = Workspace::new();
    let path = workspace.write("colors.pdf", &color_image_pdf());
    let images = extract_images(&path, &workspace.path("color-images")).expect("extract color images");
    assert_eq!(images.len(), 6);

    let cmyk = images.iter().find(|image| image.name == "Cmyk").expect("CMYK image");
    assert!(cmyk.loss.is_none(), "{:?}", cmyk.loss);
    assert_eq!((cmyk.width, cmyk.height), (1, 1));
    let png = fs::read(&cmyk.path).expect("read CMYK PNG");
    let (width, height, depth, color_type) = png_dimensions(&png);
    assert_eq!((width, height, depth, color_type), (1, 1, 8, 2));
    assert_eq!(png_scanlines(&png), vec![0, 0, 0, 0], "inverted CMYK becomes black");

    let indexed = images.iter().find(|image| image.name == "Indexed").expect("Indexed image");
    assert!(indexed.loss.is_none(), "{:?}", indexed.loss);
    let png = fs::read(&indexed.path).expect("read Indexed PNG");
    let (width, height, depth, color_type) = png_dimensions(&png);
    assert_eq!((width, height, depth, color_type), (2, 1, 8, 2));
    assert_eq!(png_scanlines(&png), vec![0, 255, 0, 0, 0, 255, 0]);

    let inverted = images.iter().find(|image| image.name == "IndexedInvert").expect("inverted Indexed image");
    assert!(inverted.loss.is_none(), "{:?}", inverted.loss);
    let png = fs::read(&inverted.path).expect("read inverted Indexed PNG");
    assert_eq!(png_scanlines(&png), vec![0, 0, 255, 0, 255, 0, 0], "/Decode reverses palette indices");

    let bad = images.iter().find(|image| image.name == "BadIndexed").expect("bad Indexed image");
    assert!(bad.path.is_empty());
    assert!(bad.loss.as_deref().unwrap_or("").contains("lookup"), "{:?}", bad.loss);

    let nibble = images.iter().find(|image| image.name == "Nibble").expect("4-bit image");
    assert!(nibble.path.is_empty());
    assert!(nibble.loss.as_deref().unwrap_or("").contains("8 bits"), "{:?}", nibble.loss);

    let icc = images.iter().find(|image| image.name == "Icc").expect("ICCBased image");
    assert!(icc.path.is_empty());
    assert!(icc.loss.as_deref().unwrap_or("").contains("Indexed"), "{:?}", icc.loss);
}

/// A one-page form with a nested text field, two buttons (one with `/AP`
/// states, one without) and a choice field with `/Opt` entries.
fn fillable_form_pdf() -> Vec<u8> {
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 8 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R 6 0 R] >>",
        "<< /T (person) /FT /Tx /Kids [5 0 R] >>",
        "<< /T (name) /Parent 4 0 R /V (Alice) >>",
        "<< /T (agree) /FT /Btn /V /Off /AP << /N << /Off 10 0 R /Yes 9 0 R >> >> >>",
        "<< /T (color) /FT /Ch /Opt [(red) (green)] >>",
        "<< /Fields [4 0 R 6 0 R 7 0 R 12 0 R] /DA (/Helv 0 Tf 0 g) /XFA 11 0 R >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 0 >>\nstream\n\nendstream",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 0 >>\nstream\n\nendstream",
        "<< /Foo (bar) >>",
        "<< /T (plain) /FT /Btn /V /Off >>",
    ])
}

fn form_field_values(report: &open_pdf_core::InspectReport) -> Vec<(String, Option<String>)> {
    report.form_fields.iter().map(|field| (field.name.clone(), field.value.clone())).collect()
}

#[test]
fn fill_form_writes_text_choice_and_button_values() {
    let workspace = Workspace::new();
    let path = workspace.write("form.pdf", &fillable_form_pdf());
    let out = workspace.path("filled.pdf");

    let source = inspect(&path).expect("inspect form");
    assert_eq!(
        form_field_values(&source),
        vec![
            ("person.name".to_string(), Some("Alice".to_string())),
            ("agree".to_string(), Some("Off".to_string())),
            ("color".to_string(), None),
            ("plain".to_string(), Some("Off".to_string())),
        ]
    );

    let mut values = BTreeMap::new();
    values.insert("person.name".to_string(), FormValue::Text("Bob Smith".to_string()));
    values.insert("agree".to_string(), FormValue::Toggle(true));
    values.insert("color".to_string(), FormValue::Text("green".to_string()));
    values.insert("plain".to_string(), FormValue::Toggle(true));
    values.insert("absent".to_string(), FormValue::Text("nothing".to_string()));

    let report = fill_form(&path, &out, &values).expect("fill form");
    assert_eq!(report.pages, 1);
    assert_eq!(report.path, out.display().to_string());
    assert_eq!(report.filled, ["agree", "color", "person.name", "plain"]);
    assert_eq!(report.missing, ["absent"]);

    let filled = inspect(&out).expect("inspect filled form");
    assert_eq!(
        form_field_values(&filled),
        vec![
            ("person.name".to_string(), Some("Bob Smith".to_string())),
            ("agree".to_string(), Some("Yes".to_string())),
            ("color".to_string(), Some("green".to_string())),
            ("plain".to_string(), Some("Yes".to_string())),
        ]
    );

    let text = String::from_utf8_lossy(&fs::read(&out).expect("read filled PDF")).into_owned();
    assert!(text.contains("/NeedAppearances true"), "NeedAppearances is missing");
    assert!(text.contains("/XFA"), "the existing XFA key was dropped");
    assert!(!text.contains("(Alice)"), "the old text value was kept");
}

#[test]
fn fill_form_turns_buttons_off_and_keeps_unknown_names_in_missing() {
    let workspace = Workspace::new();
    let path = workspace.write("form.pdf", &fillable_form_pdf());
    let out = workspace.path("cleared.pdf");

    let mut values = BTreeMap::new();
    values.insert("agree".to_string(), FormValue::Toggle(false));
    values.insert("person.name".to_string(), FormValue::Text(String::new()));
    values.insert("person.unknown".to_string(), FormValue::Text("x".to_string()));

    let report = fill_form(&path, &out, &values).expect("fill form");
    assert_eq!(report.filled, ["agree", "person.name"]);
    assert_eq!(report.missing, ["person.unknown"]);

    let filled = inspect(&out).expect("inspect filled form");
    let fields = form_field_values(&filled);
    assert_eq!(fields[0], ("person.name".to_string(), Some(String::new())));
    assert_eq!(fields[1], ("agree".to_string(), Some("Off".to_string())));
}

#[test]
fn fill_form_rejects_empty_values_and_blank_field_names() {
    let workspace = Workspace::new();
    let path = workspace.write("form.pdf", &fillable_form_pdf());
    let out = workspace.path("rejected.pdf");

    assert_invalid(fill_form(&path, &out, &BTreeMap::new()), "session/argument-invalid");

    let mut blank = BTreeMap::new();
    blank.insert("   ".to_string(), FormValue::Text("x".to_string()));
    assert_invalid(fill_form(&path, &out, &blank), "session/argument-invalid");
}


#[test]
fn merge_selected_keeps_only_the_requested_pages_per_input() {
    let workspace = Workspace::new();
    let first = workspace.create("first.pdf", Some("First"), &["AAA", "BBB", "CCC"]);
    let second = workspace.create("second.pdf", Some("Second"), &["DDD", "EEE"]);
    let merged = workspace.path("selected.pdf");

    let report = merge_selected(
        &[first, second],
        Some(&["3,1".to_string()]),
        &merged,
        false,
        None,
    )
    .expect("merge selections");
    assert_eq!(report["pages"], json!(4));
    assert_eq!(report["sources"][0]["selected"], json!(2));
    assert_eq!(report["sources"][0]["ranges"], json!("3,1"));
    assert_eq!(report["sources"][1]["selected"], json!(2));
    assert_eq!(report["sources"][1]["ranges"], json!(null));

    let inspected = inspect(&merged).expect("inspect merged");
    assert_eq!(inspected.pages, 4);
    let extracted = extract_text(&merged).expect("extract merged").markdown;
    let aaa = extracted.find("AAA").expect("AAA");
    let ccc = extracted.find("CCC").expect("CCC");
    let ddd = extracted.find("DDD").expect("DDD");
    let eee = extracted.find("EEE").expect("EEE");
    assert!(ccc < aaa && aaa < ddd && ddd < eee, "selection order is not preserved: {extracted}");
    assert!(!extracted.contains("BBB"), "the dropped page is gone: {extracted}");
}

#[test]
fn merge_selected_rejects_bad_ranges_and_existing_output() {
    let workspace = Workspace::new();
    let first = workspace.create("first.pdf", None, &["AAA", "BBB"]);
    let merged = workspace.path("selected.pdf");

    assert_invalid(
        merge_selected(&[first.clone()], Some(&["3".into()]), &merged, false, None),
        "session/argument-invalid",
    );
    assert_invalid(
        merge_selected(&[first.clone()], Some(&["".into()]), &merged, false, None),
        "session/argument-invalid",
    );
    assert_invalid(
        merge_selected(&[first.clone()], Some(&["2-1".into()]), &merged, false, None),
        "session/argument-invalid",
    );
    assert_invalid(
        merge_selected(&[first.clone()], Some(&["1".into(), "2".into()]), &merged, false, None),
        "session/argument-invalid",
    );

    fs::write(&merged, b"placeholder").expect("seed output");
    assert_invalid(merge_selected(&[first], None, &merged, false, None), "io/exists");
    let report = merge_selected(&[workspace.create("other.pdf", None, &["CCC"])], None, &merged, true, None)
        .expect("overwrite replaces the output");
    assert_eq!(report["pages"], json!(1));
}

#[test]
fn clear_metadata_empties_every_standard_field() {
    let workspace = Workspace::new();
    let source = workspace.path("tagged.pdf");
    let report = create(&CreateRequest {
        out: source.clone(),
        title: Some("Tagged".to_owned()),
        pages: vec!["Hello".to_owned()],
        overwrite: false,
        page_size: None,
        font: None,
        font_size: None,
        margin_pt: None,
        align: None,
        author: Some("Ada".to_owned()),
        subject: Some("Notes".to_owned()),
        keywords: Some("one, two".to_owned()),
        creator: Some("Tests".to_owned()),
        page_numbers: None,
    })
    .expect("create tagged PDF");
    assert_eq!(report.pages, 1);
    let before = inspect(&source).expect("inspect source");
    assert_eq!(before.metadata.title.as_deref(), Some("Tagged"));
    assert_eq!(before.metadata.author.as_deref(), Some("Ada"));

    let cleared = workspace.path("cleared.pdf");
    let report = clear_metadata(&source, &cleared).expect("clear metadata");
    assert_eq!(report.pages, 1);
    assert_eq!(report.path, cleared.display().to_string());
    for field in [
        report.metadata.title.as_deref(),
        report.metadata.author.as_deref(),
        report.metadata.subject.as_deref(),
        report.metadata.keywords.as_deref(),
        report.metadata.creator.as_deref(),
        report.metadata.producer.as_deref(),
    ] {
        assert_eq!(field, None, "every standard field is cleared");
    }

    let inspected = inspect(&cleared).expect("inspect cleared");
    assert_eq!(inspected.pages, 1);
    assert_eq!(inspected.metadata.title, None);
    assert_eq!(extract_text(&cleared).expect("text survives").markdown.contains("Hello"), true);

    clear_metadata(&source, &cleared).expect("replacing the output is the documented behaviour");
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(cleared);
}

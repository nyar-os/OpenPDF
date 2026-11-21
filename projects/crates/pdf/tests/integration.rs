use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use open_pdf_core::{CreateRequest, create, extract_text, inspect, merge, split};

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

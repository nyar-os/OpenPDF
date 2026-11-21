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

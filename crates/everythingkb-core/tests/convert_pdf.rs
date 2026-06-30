use std::path::Path;

#[test]
fn convert_pdf_document() {
    let path = Path::new("/home/qhu/Documents/gellm/s41467-023-36635-5.pdf");
    if !path.exists() {
        return;
    }
    let kb = everythingkb_core::KbPaths::open(None).expect("kb");
    let reg = kb.open_registry().expect("registry");
    let r = everythingkb_core::convert::convert_document(path, &kb, &reg, true, false)
        .expect("convert");
    let src = r.source_path.expect("source path");
    let content = std::fs::read_to_string(&src).expect("read source");
    eprintln!("source {} bytes at {}", content.len(), src.display());
    assert!(
        content.len() > 1000,
        "expected extracted text, got: {:?}",
        &content[..content.len().min(200)]
    );
}

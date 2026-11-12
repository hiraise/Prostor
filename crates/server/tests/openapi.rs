//! The committed API document matches the code, so the web app's generated types are current.
//! `make api` rewrites the document and regenerates the TypeScript types.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

const DOCUMENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/lib/api/openapi.json"
);

#[test]
fn int_056_the_committed_api_document_is_current() {
    let current = serde_json::to_string_pretty(&trackflow_server::openapi()).unwrap() + "\n";
    if std::env::var_os("TRACKFLOW_UPDATE_API").is_some() {
        std::fs::write(DOCUMENT, &current).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(DOCUMENT).unwrap_or_default();
    assert!(
        committed == current,
        "the API changed: run `make api` and commit web/src/lib/api/"
    );
}

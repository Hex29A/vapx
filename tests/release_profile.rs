//! A panic inside one `vapx mcp` tool call is caught by the server and answered as an error
//! (see `call_tool` in `src/mcp/server.rs`), but only when panics unwind. With `panic = "abort"` in
//! the release profile the whole server would end instead, so keep that setting out.

#[test]
fn the_release_profile_unwinds_on_panic() {
    let manifest = include_str!("../Cargo.toml");
    let release = manifest.split("[profile.release]").nth(1).expect("a release profile");
    let release = release.split("\n[").next().unwrap();
    assert!(!release.contains("abort"), "panic = \"abort\" in [profile.release] would let one tool panic end vapx mcp:\n{release}");
}

# GPUI Windows backend for Silence

Upstream: https://github.com/zed-industries/zed/tree/a84689073d296dfd39987bc7dd478e43ef76d83a/crates/gpui_windows
Revision: `a84689073d296dfd39987bc7dd478e43ef76d83a`.
License: Apache-2.0, included in `LICENSE-APACHE`.

This is the Windows backend only. GPUI core and its utilities stay on the same
upstream revision. Workspace-inherited dependency metadata is materialized here
so the crate builds independently of the Zed workspace.

Silence enables the `small-overlay` feature for its small, fixed-size overlay:

- Create shader/instance pipelines only when their primitive is first uploaded.
- Create color-glyph GPU resources only when rendering a color glyph.
- Load the system font collection on first font lookup/enumeration.
- Start sprite atlases at 128x128 and allocate larger pages when necessary.
- Request fewer internal Direct3D/driver worker threads, retaining normal D3D11
  device thread safety. This trades driver parallelism for memory and is intended
  for the tiny overlay, not arbitrary large GPUI applications.
- Clear bound D3D11 state and flush once at renderer teardown.

Independently of that feature, `WM_ENDSESSION` no longer quits GPUI and calls
`std::process::exit` from the overlay thread. That ran DLL detach for every
module while the main thread and D3D device were still live and crashed on
Windows shutdown (issue #68); the OS now ends the process as before the overlay.

Frame requests are demand-driven: windows implement `PlatformWindow::frame_waker`,
and the `VSyncProvider` thread sleeps until GPUI asks for a frame instead of
invalidating every window on every vsync (issue #69).

Modified upstream files: `src/directx_renderer.rs`, `src/direct_write.rs`,
`src/directx_atlas.rs`, `src/directx_devices.rs`, `src/events.rs`,
`src/platform.rs`, `src/vsync.rs`, and `src/window.rs`. Other source/build files
are unchanged from the revision above. The default profile keeps the upstream
atlas size, eager resources and driver-thread policy.

See `../../docs/research/gpui-memory.md` for source citations, measurements,
limitations and the experiment history. Updating upstream requires reapplying
and revalidating these changes.

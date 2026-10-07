# Android (launch to menu)

Minimal scaffolding: the menu boots on device. In-match play is not vetted.

## Build

Install the SDK (platform 33, build-tools) and NDK, set `ANDROID_HOME` and
`ANDROID_NDK_ROOT`, then:

```sh
rustup target add aarch64-linux-android
cargo install cargo-apk
cargo apk build -p launcher --lib --release   # target/release/apk/iw4l.apk
cargo apk run -p launcher --lib --release     # build, install, launch over adb
```

`--release` needs a keystore; for sideloading the SDK debug key does:
`CARGO_APK_RELEASE_KEYSTORE=~/.android/debug.keystore` and
`CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android`.

cargo-apk sets the NDK compilers itself; a bare `cargo check --target
aarch64-linux-android` needs `CC_`/`CXX_`/`AR_aarch64_linux_android` pointing
at the NDK's `aarch64-linux-android29-clang(++).cmd` and `llvm-ar.exe`.
xbuild does not fit: it ignores the lib name and `[package.metadata.android]`.

The cdylib is `libiw4l.so` (`crates/launcher`, `android.app.lib_name=iw4l`).
cargo-apk builds the manifest from `[package.metadata.android]` in
`crates/launcher/Cargo.toml` (landscape, permissions);
`crates/launcher/AndroidManifest.xml` mirrors it for other packagers.
Bevy's `android-native-activity` feature is enabled for the android target only.

## On-device paths (`$ANDROID_PRIVATE`, else `/data/data/com.iw4l/files`)

| What | Where |
| --- | --- |
| games root | `<private>/games` (`IW4L_GAMES` overrides; `adb push` the trees) |
| artifacts + logs | `<private>/iw4l-artifacts` (`IW4L_ARTIFACTS_DIR` overrides) |
| settings + account | `<private>/iw4l-artifacts/settings.cfg`, `account.dat` |

Present mode is forced to `Fifo`; `IW4L_PRESENT_MODE` is ignored.
Only base wgpu features are requested; desktop extras (BC, wireframe, bindless,
16-bit norm) are skipped, and use sites must check `device.features()` first.
Mobile GPUs cannot sample BC (DXT), so `asset_material` expands material
textures to RGBA8 on the decoding worker (`DecodedMips::for_device`). To stay
within a phone's memory they also drop top mips until their largest side is at
most 256 px (`EXPANDED_MAX_SIDE`). The mip cache stays compressed and full size,
and UI images (menu art, map previews) are not capped. At 512 px, mp_abandon was
killed at about 7 GB on an 8 GB phone.
On Android a load keeps nothing for the next one (`KEEP_FOR_NEXT_LOAD` in
`assets::session_load`): the match takes the common set instead of copying it,
and there is no resident map or kept first-person texels, so a second load walks
`common_mp` again. On Android, `load stage:` lines carry `rss=` and `swap=`
deltas; once the phone swaps, resident bytes alone undercount. The heap figure
needs `IW4L_COUNTING_ALLOC`.

## Frame rate

Phones default to lighter graphics (`frame::GameSettings`):

| Setting (`settings.cfg`) | Phone | Desktop | Override |
| --- | --- | --- | --- |
| `render_scale` | 0.667 | 1.0 | `IW4L_RENDER_SCALE` |
| `max_fps` | 30 | 0 (vsync only) | `IW4L_MAX_FPS` |
| `shadows`, `depth_of_field`, `bloom` | off | on | menu, `sm_enable`, `r_dof_enable`, `r_glow` |

Below 1.0 the lens renders into an image that a camera under the HUD stretches
over the window (`bootstrap::render_scale`); the HUD stays at full resolution
and projects through `Hud2dSurface::project`. `max_fps` sleeps the main world to
a fixed grid (`bootstrap::frame_pacing`): on a 60 Hz `Fifo` panel a frame time
near 16.7 ms would otherwise flip between 60 and 30. Settings files older than
v3 keep the phone's graphics defaults instead of the desktop ones they were
written with. `shadows` gates sun shadows as well as spot shadows.

The allocator is mimalloc, as on Windows (`diag`); it needs `local_dynamic_tls`
because the game is a dlopened library. Animated script models farther than
768 units keep their last CPU-skinned pose for 2 frames, past 2048 for 4
(`HOLD_DISTANT_POSES` in `render_anim`).

Every 5 s the log has a `frames:` line: `p50/p95/p99/max` of the frame interval,
main-world `update`, render-world `render` (without `acquire`) and the
swapchain `acquire` wait, which grows when the GPU or vsync is the limit.
`IW4L_FRAME_LOG=1` turns it on off Android.

## Game files on SD card

Game installs (folders with a `zone/` subfolder) up to three folders deep on
internal storage or any SD card are found at startup, e.g.
`/storage/XXXX-XXXX/Download/Call of Duty - Modern Warfare 2`. SD cards come
from `/proc/self/mounts`: apps may not list `/storage`. The log names the
volumes scanned and the installs found.
Precedence: `IW4L_GAMES` > `ANDROID_SDCARD_GAMES` > `<private>/games`;
`settings.cfg` `game_folder_<key>` entries add per-game search roots.
Example: `ANDROID_SDCARD_GAMES=/storage/XXXX-XXXX/MW2`.
Grant "All files access" to iw4l in system settings (game files are not media,
so the Files/Media grant alone cannot read them), then pick each folder in
Settings > Game Folders > Browse, and restart: folder changes load at startup
only. Browse opens an in-game browser over Internal storage and SD cards (not
SAF: its `content://` URIs are unreadable by the asset loader). From the
console: `set ui_set_game_folder black_ops=/storage/XXXX-XXXX/BlackOps`.

## Known limits

- No Steam linking or shortcut (`.lnk`) discovery.
- Touch input is minimal (menu only).
- Gamepads: gilrs has no Android backend, and winit reads sticks as touches.
  `bootstrap::android_gamepad` claims controller events through the input
  filter added to the vendored `third_party/android-activity` (see the root
  `Cargo.toml`) and feeds Bevy's gamepad events. Android reports no disconnect
  through the input queue, so a pad stays registered until restart.
- Updater is inert (no download/replace off Windows); master browser stays
  disabled unless its env is set.

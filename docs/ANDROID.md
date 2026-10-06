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
- Touch input is minimal (menu only); gamepad binds are desktop-owned.
- Updater is inert (no download/replace off Windows); master browser stays
  disabled unless its env is set.

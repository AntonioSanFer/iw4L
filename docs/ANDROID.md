# Android (launch to menu)

Minimal scaffolding: the menu boots on device. In-match play is not vetted.

## Build

Install the NDK (r25+, via `ANDROID_NDK_ROOT`), then:

```sh
rustup target add aarch64-linux-android
cargo install xbuild # or cargo-apk (deprecated upstream)
xbuild run -p launcher # or: cargo apk run -p launcher
```

The cdylib is `libiw4l.so` (`crates/launcher`, `android.app.lib_name=iw4l`);
the manifest is `crates/launcher/AndroidManifest.xml` (landscape).
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

Precedence: `IW4L_GAMES` > `ANDROID_SDCARD_GAMES` > `<private>/games`;
`settings.cfg` `game_folder_<key>` entries add per-game search roots.
Example: `ANDROID_SDCARD_GAMES=/storage/XXXX-XXXX/MW2`.
Grant Files/Media permission in system settings, then set a folder, e.g.
`set ui_set_game_folder black_ops=/storage/XXXX-XXXX/BlackOps`, and restart:
folder changes load at startup only. Removable SD may also need an
all-files grant; no in-app picker (no SAF) — paste real `/storage/` paths only.

## Known limits

- No Steam linking or shortcut (`.lnk`) discovery; no folder picker —
  place game trees under the games root before launch.
- Touch input is minimal (menu only); gamepad binds are desktop-owned.
- Updater is inert (no download/replace off Windows); master browser stays
  disabled unless its env is set.

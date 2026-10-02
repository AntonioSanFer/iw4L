# Portable Windows folder

Players download `iw4l-windows.zip` from the GitHub release; it holds the executable
and licences. A server operator hands out the `.iw4l-server` descriptor separately.
`make release prod|dev` also writes `iw4l-windows-{dev,prod}.zip` with the descriptor
included, and `make launcher windows` writes the same pair under `dist/windows/`.
The archive password is `t.me/contextrot`. Extract into a dedicated writable folder:

```text
IW4L/
├── iw4l.exe
├── community.iw4l-server
├── LICENSE NOTICE OFL-Oxanium.txt COPYING-FreeFont.txt
├── Modern Warfare 2.lnk
├── Black Ops.lnk          optional
├── Modern Warfare 3.lnk   optional
└── iw4l-artifacts/        saves, caches, demos, logs and captures
```

Add ordinary Windows shortcuts to installed title folders or executables.
MW2 multiplayer data is required for the menu; BO1 and MW3 are optional.
The runtime reads those installations.

Launch `iw4l.exe`. Before starting the game or contacting QUIC, it checks its
community's HTTPS manifest. An unchanged executable starts normally. An update
is downloaded, decompressed with a size limit and verified against SHA-256.
A temporary copy of the same executable waits for the original process, replaces
it and restarts with the original arguments. Failed installation or spawning
restores the previous executable. There is no permanent second executable.

The temporary files are `iw4l.update.exe`, `iw4l.previous.exe` and a helper in
`%TEMP%`; the next verified startup removes the backup and helper. The lock file
`iw4l.update.lock` serializes update checks; `iw4l.update-helper` records cleanup.
An update error stops startup and reports the failure; a configured community requires a reachable update origin.
Without a community descriptor, local development can launch without checking.

A descriptor pins a public CA and the master's TLS name for both HTTPS and QUIC.
Use a descriptor from a trusted source: its operator can distribute executable
updates. Its settings take precedence over the legacy master environment keys.
One adjacent `.iw4l-server` is selected automatically; with several, set
`IW4L_COMMUNITY` to the chosen file path before launching. See [`MASTER.md`](MASTER.md).

On Windows the executable directory is the working directory. Game discovery
uses `IW4L_GAMES` or shortcuts beside `iw4l.exe`; local `.env` settings remain
available for game configuration. Publishing: [`DEPLOY.md`](DEPLOY.md).

# Deploying with systemd

This guide shows how to run AUR Mirror Meta (AMM) as a long-running service:

- A **serve** unit that keeps the HTTP RPC server up at all times.
- A **sync** unit triggered by a **timer** on a fixed interval (default **6h**) with
  randomized jitter so that, when many mirrors are deployed, they don't all hit
  GitHub / AUR at the exact same moment.

## 1. Install the binary

```bash
cargo build --release
sudo install -Dm755 target/release/aur-mirror-meta /usr/local/bin/aur-mirror-meta
```

## 2. Provide the GitHub token (optional but recommended)

Logging in via `aur-mirror-meta login` writes the token into a per-user config
file, which is awkward for a system service. Instead, pass it through the
environment. Create an environment file readable only by root:

```bash
sudo install -Dm600 /dev/stdin /etc/aur-mirror-meta.env <<'EOF'
AMM_GITHUB_TOKEN=ghp_your_personal_access_token
EOF
```

If you don't want to use a token, you can skip this step and drop the
`EnvironmentFile=` line from the units below.

## 3. Shared service settings

Both units run as a sandboxed `DynamicUser` and share a state directory at
`/var/lib/aur-mirror-meta` (created automatically by `StateDirectory=`). The
database is pinned there via `AMM_DB_PATH` so `serve` and `sync` agree on its
location.

## 4. The serve service

`/etc/systemd/system/aur-mirror-meta-serve.service`

```ini
[Unit]
Description=AUR Mirror Meta - HTTP RPC server
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
DynamicUser=yes
StateDirectory=aur-mirror-meta
Environment=AMM_DB_PATH=/var/lib/aur-mirror-meta/aur-meta.db
EnvironmentFile=-/etc/aur-mirror-meta.env
ExecStart=/usr/local/bin/aur-mirror-meta serve --bind [::]:3000
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

Adjust `--bind` to taste. The default is `[::]:3000` (listens on all IPv6/IPv4
addresses). You may pass `--bind` multiple times to listen on several addresses,
e.g. `--bind 127.0.0.1:3000 --bind [::1]:3000`.

## 5. The sync service

A `oneshot` service that runs a single sync and exits. The timer (next section)
drives it on a schedule.

`/etc/systemd/system/aur-mirror-meta-sync.service`

```ini
[Unit]
Description=AUR Mirror Meta - sync metadata
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
DynamicUser=yes
StateDirectory=aur-mirror-meta
Environment=AMM_DB_PATH=/var/lib/aur-mirror-meta/aur-meta.db
EnvironmentFile=-/etc/aur-mirror-meta.env
ExecStart=/usr/local/bin/aur-mirror-meta sync
```

## 6. The sync timer

`/etc/systemd/system/aur-mirror-meta-sync.timer`

```ini
[Unit]
Description=AUR Mirror Meta - sync metadata every 6h (with jitter)

[Timer]
# First run shortly after boot...
OnBootSec=15min
# ...then every 6 hours after the previous run finished.
OnUnitActiveSec=6h
# Spread the actual trigger over a window of up to 30 minutes so multiple
# mirrors don't stampede GitHub / AUR at the same instant.
RandomizedDelaySec=30min
# Let systemd coalesce wake-ups within this slack for efficiency.
AccuracySec=1min
Persistent=true

[Install]
WantedBy=timers.target
```

Notes on the jitter:

- `RandomizedDelaySec=30min` adds a random delay of 0–30 minutes to each trigger.
  This is the jitter requested for the default 6h cadence; tune it to a fraction
  of your interval.
- `OnUnitActiveSec=6h` schedules each run relative to the **previous activation**,
  so a long sync won't cause overlapping runs.
- `Persistent=true` makes systemd run a missed sync after downtime (e.g. the
  machine was off when a trigger was due).

## 7. Enable everything

```bash
sudo systemctl daemon-reload

# Start and enable the server
sudo systemctl enable --now aur-mirror-meta-serve.service

# Enable the periodic sync timer (this also schedules the sync service)
sudo systemctl enable --now aur-mirror-meta-sync.timer

# Optionally, run a sync immediately to populate the database the first time
sudo systemctl start aur-mirror-meta-sync.service
```

## 8. Operating

```bash
# Watch the server / sync logs
journalctl -u aur-mirror-meta-serve.service -f
journalctl -u aur-mirror-meta-sync.service -f

# See when the next sync is scheduled and the last run result
systemctl list-timers aur-mirror-meta-sync.timer
systemctl status aur-mirror-meta-sync.service

# Trigger a sync on demand
sudo systemctl start aur-mirror-meta-sync.service
```
